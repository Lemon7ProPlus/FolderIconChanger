// src/config_store.rs

use std::sync::mpsc;
use std::time::Duration;
use std::thread;

use crate::types::AppConfig;

pub struct ConfigStore {
    tx: mpsc::Sender<AppConfig>,
}

impl ConfigStore {
    pub fn new(file_path: &'static str) -> Self {
        let (tx, rx) = mpsc::channel::<AppConfig>();
        thread::spawn(move || {
            let mut pending_config: Option<AppConfig> = None;
            loop {
                match rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(new_config) => {
                        pending_config = Some(new_config);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Some(cfg) = pending_config.take() {
                            if let Ok(toml_str) = toml::to_string(&cfg) {
                                let _ = std::fs::write(file_path, toml_str);
                            }
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        Self {tx}
    }

    pub fn save_debounced(&self, config:AppConfig) {
        let _ = self.tx.send(config);
    }
}