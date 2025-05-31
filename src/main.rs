use chrono::Local;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Result, Watcher};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

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

fn listen(rx: Receiver<Result<Event>>) {
    loop {
        let event = rx.recv().unwrap();
        match event {
            Ok(event) => handle(event),
            Err(e) => println!("Watch error: {:?}", e),
        }
    }
}

fn watch(watch_path: &str, tx: Sender<Result<Event>>) {
    let mut watcher = Box::new(
        RecommendedWatcher::new(
            tx,
            Config::default().with_poll_interval(Duration::from_secs(1)),
        )
        .unwrap(),
    );

    watcher
        .watch(Path::new(watch_path), RecursiveMode::Recursive)
        .unwrap();

    Box::leak(watcher);
}

fn main() {
    let watch_path = "~/code/opensearch-spark/target/test-reports";

    eprintln!("Starting directory watcher for: {}", watch_path);

    let (tx, rx) = channel();
    watch(watch_path, tx);

    eprintln!("Press Ctrl+C to stop...");

    listen(rx);
}
