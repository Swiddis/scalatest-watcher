use anyhow::Context;
use junit_parser::TestSuites;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

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

fn handle(event: Event, suites: &mut HashMap<String, TestSuites>) -> Result<(), anyhow::Error> {
    match event.kind {
        EventKind::Modify(_) => {
            for path in event.paths {
                if !path.is_file() {
                    continue;
                }

                update_suite(&path, suites, false)?;
            }
        }
        // We need this to be distinct from `Modify` to avoid an infinite loop
        // of Access events causing new Access events
        EventKind::Create(_) | EventKind::Access(_) => {
            for path in event.paths {
                if !path.is_file() {
                    continue;
                }

                update_suite(&path, suites, true)?;
            }
        }
        EventKind::Remove(_) => {
            for path in event.paths {
                suites.remove(&path.display().to_string());
            }
        }
        _ => {
            // no-op
        }
    }

    Ok(())
}

fn listen(rx: Receiver<notify::Result<Event>>) {
    let mut suites: HashMap<String, TestSuites> = HashMap::new();

    loop {
        let event = rx.recv().unwrap();
        match event {
            Ok(event) => handle(event, &mut suites).expect("io error while handling event"),
            Err(e) => println!("Watch error: {:?}", e),
        }
    }
}

fn watch(config: &WatchConfig, tx: Sender<notify::Result<Event>>) {
    let mut watcher = Box::new(RecommendedWatcher::new(tx, Config::default()).unwrap());
    watcher
        .watch(Path::new(&config.path), RecursiveMode::Recursive)
        .unwrap();
    Box::leak(watcher);

    // Generate access events to register all files with the listener
    for entry in walkdir::WalkDir::new(&config.path).into_iter().flatten() {
        if entry.metadata().unwrap().is_file() {
            let _ = OpenOptions::new().read(true).open(entry.path());
        }
    }
}

fn load_config() -> Result<WatchConfig, config::ConfigError> {
    let base_path = std::env::current_dir().expect("Failed to determine the current directory");

    let settings = config::Config::builder()
        .add_source(config::File::from(base_path.join("config.toml")))
        .add_source(config::Environment::with_prefix("SC_WATCH_"))
        .build()?;
    settings.try_deserialize::<WatchConfig>()
}

fn main() {
    let config = load_config().unwrap();

    eprintln!("Starting directory watcher for: {}", config.path);

    let (tx, rx) = channel();
    watch(&config, tx);

    eprintln!("Press Ctrl+C to stop...");

    listen(rx);
}
