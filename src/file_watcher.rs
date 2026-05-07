// src/file_watcher.rs

use std::{path::Path, sync::mpsc};

use notify::{Config as NotifyConfig, Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::types::AppConfig;

pub fn start_watching(
    file_path: &'static str, 
    tx: mpsc::Sender<Result<AppConfig, String>>,
    waker: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        let (notify_tx, notify_rx) = std::sync::mpsc::channel();
        let mut watcher = RecommendedWatcher::new(notify_tx, NotifyConfig::default()).unwrap();
        let _ = watcher.watch(Path::new(file_path), RecursiveMode::NonRecursive);

        for res in notify_rx {
            if let Ok(Event { kind: notify::EventKind::Modify(_), .. }) = res {
                std::thread::sleep(std::time::Duration::from_millis(50));
                if let Ok(data) = std::fs::read_to_string(file_path) {
                    // 解析 TOML，成功发 Ok，失败发 Err 携带错误原因
                    match toml::from_str(&data) {
                        Ok(config) => { let _ = tx.send(Ok(config)); }
                        Err(e) => { let _ = tx.send(Err(e.to_string())); }
                    }
                    waker(); 
                }
            }
        }
    });
}