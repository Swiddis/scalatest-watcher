use anyhow::Context;
use axum::http::StatusCode;
use junit_parser::TestSuites;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use tokio::sync::broadcast;

#[derive(serde::Deserialize, Clone)]
struct WatchConfig {
    pub path: String,
}

fn parse_suite(path: &PathBuf) -> Result<TestSuites, anyhow::Error> {
    let xml = std::fs::read_to_string(path).context("failed to load JUnit suite data")?;
    let cursor = Cursor::new(xml);
    junit_parser::from_reader(cursor).context("failed to parse JUnit XML")
}

fn update_suite(
    path: &PathBuf,
    suites: &mut HashMap<String, TestSuites>,
    skip_if_absent: bool,
) -> Result<(), anyhow::Error> {
    let display = path.display().to_string();
    if skip_if_absent && suites.contains_key(&display) {
        return Ok(());
    }

    let suite = parse_suite(path)?;
    suites.insert(display, suite);
    Ok(())
}

fn handle(event: &Event, suites: &mut HashMap<String, TestSuites>) -> Result<(), anyhow::Error> {
    match event.kind {
        EventKind::Modify(_) => {
            for path in event.paths.iter() {
                if !path.is_file() {
                    continue;
                }

                update_suite(path, suites, false)?;
            }
        }
        // We need this to be distinct from `Modify` to avoid an infinite loop
        // of Access events causing new Access events
        EventKind::Create(_) | EventKind::Access(_) => {
            for path in event.paths.iter() {
                if !path.is_file() {
                    continue;
                }

                update_suite(path, suites, true)?;
            }
        }
        EventKind::Remove(_) => {
            for path in event.paths.iter() {
                suites.remove(&path.display().to_string());
            }
        }
        _ => {
            // no-op
        }
    }

    Ok(())
}

fn listen(rx: Receiver<notify::Result<Event>>, tx_ws: broadcast::Sender<String>) {
    let mut suites: HashMap<String, TestSuites> = HashMap::new();

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                if let Ok(_) = handle(&event, &mut suites) {
                    for path in event.paths {
                        let key = path.display().to_string();
                        if let Some(suite) = suites.get(&key) {
                            if let Ok(json) = serde_json::to_string(&(key, suite)) {
                                let _ = tx_ws.send(json);
                            }
                        }
                    }
                }
            }
            Ok(Err(e)) => eprintln!("Watch error: {:?}", e),
            Err(_) => break,
        }
    }
}

async fn ws_handler(
    ws: axum::extract::ws::WebSocketUpgrade,
    tx_ws: broadcast::Sender<String>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |mut socket| async move {
        let mut rx = tx_ws.subscribe();
        while let Ok(msg) = rx.recv().await {
            if socket
                .send(axum::extract::ws::Message::Text(msg.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}

/// Generate access events to register all files with the listener
fn start_refresh(config: WatchConfig) -> impl axum::response::IntoResponse {
    let conf_clone = config.clone();
    std::thread::spawn(move || {
        for entry in walkdir::WalkDir::new(conf_clone.path).into_iter().flatten() {
            if entry.metadata().unwrap().is_file() {
                let _ = OpenOptions::new().read(true).open(entry.path());
            }
        }
    });

    StatusCode::CREATED
}

fn watch(config: WatchConfig, tx: Sender<notify::Result<Event>>) {
    let mut watcher = Box::new(RecommendedWatcher::new(tx, Config::default()).unwrap());
    watcher
        .watch(Path::new(&config.path), RecursiveMode::Recursive)
        .unwrap();
    Box::leak(watcher);
    start_refresh(config);
}

fn load_config() -> Result<WatchConfig, config::ConfigError> {
    let base_path = std::env::current_dir().expect("Failed to determine the current directory");

    let settings = config::Config::builder()
        .add_source(config::File::from(base_path.join("config.toml")))
        .add_source(config::Environment::with_prefix("SC_WATCH_"))
        .build()?;
    settings.try_deserialize::<WatchConfig>()
}

#[tokio::main]
async fn main() {
    let config = load_config().unwrap();

    eprintln!("Starting directory watcher for: {}", config.path);

    let (tx_file, rx_file) = channel();
    let (tx_ws, _) = broadcast::channel::<String>(16);

    let conf_clone = config.clone();
    std::thread::spawn(move || {
        watch(conf_clone, tx_file);
    });

    let tx_ws_clone = tx_ws.clone();
    std::thread::spawn(move || {
        listen(rx_file, tx_ws_clone);
    });

    let serve_dir = tower_http::services::ServeDir::new("frontend");

    let conf_clone = config.clone();
    let app: axum::Router = axum::Router::new()
        .route(
            "/refresh",
            axum::routing::post(async || {
                start_refresh(conf_clone)
            }),
        )
        .route("/ws", axum::routing::get(move |ws| ws_handler(ws, tx_ws)))
        .fallback_service(serve_dir);

    println!("Server running at http://localhost:3000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
