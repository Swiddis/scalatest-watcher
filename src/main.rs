use chrono::Local;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};

#[derive(serde::Deserialize, Clone)]
struct WatchConfig {
    pub path: String,
}

fn handle(event: Event) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    match event.kind {
        EventKind::Create(_) => {
            println!("[{}] Created: {:?}", timestamp, event.paths);
        }
        EventKind::Modify(_) => {
            println!("[{}] Modified: {:?}", timestamp, event.paths);
        }
        EventKind::Remove(_) => {
            println!("[{}] Removed: {:?}", timestamp, event.paths);
        }
        EventKind::Access(_) => {
            println!("[{}] Accessed: {:?}", timestamp, event.paths);
        }
        _ => {
            // no-op
        }
    }
}

fn listen(rx: Receiver<notify::Result<Event>>) {
    loop {
        let event = rx.recv().unwrap();
        match event {
            Ok(event) => handle(event),
            Err(e) => println!("Watch error: {:?}", e),
        }
    }
}

fn watch(config: &WatchConfig, tx: Sender<notify::Result<Event>>) {
    let mut watcher = Box::new(
        RecommendedWatcher::new(
            tx,
            Config::default(),
        )
        .unwrap(),
    );

    watcher
        .watch(Path::new(&config.path), RecursiveMode::Recursive)
        .unwrap();

    Box::leak(watcher);
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
