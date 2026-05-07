// src/main.rs

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use eframe::egui;
use crate::gui::FolderIconApp;

mod icon_extractor;
mod types;
mod utils;
mod gui;
mod config_store;
mod file_watcher;
mod icon_cache;
mod app_state;

use std::fs;
use std::sync::mpsc;

pub const CONFIG_FILE: &str = "mappings.toml";

fn main() -> eframe::Result<()> {
    // 1. 尝试读取现有配置
    let initial_config = fs::read_to_string(CONFIG_FILE)
        .ok()
        .and_then(|data| toml::from_str(&data).ok())
        .unwrap_or_default();
    // 2. 启动配置热更新监听 (Watcher)
    let (watcher_tx, watcher_rx) = mpsc::channel();
    file_watcher::start_watching(CONFIG_FILE, watcher_tx);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(FolderIconApp::load_icon())
            .with_inner_size([500.0, 460.0])
            .with_min_inner_size([400.0, 295.0])
            .with_title("Folder Icon Changer"),
        renderer: eframe::Renderer::Glow,   // 切换到Glow，相比Wgpu节省内存
        ..Default::default()
    };
    
    eframe::run_native(
        "Folder Icon Changer",
        options,
        Box::new(|cc| {
            Ok(Box::new(gui::FolderIconApp::new(cc, initial_config, watcher_rx)))
        }),
    )
}
