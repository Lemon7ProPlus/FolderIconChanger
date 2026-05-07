// src/main.rs

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use eframe::egui;
use std::fs;
use crate::gui::FolderIconApp;

mod icon_extractor;
mod types;
mod utils;
mod gui;
mod config_store;
mod file_watcher;
mod icon_provider;
mod app_state;

pub const CONFIG_FILE: &str = "mappings.toml";

fn main() -> eframe::Result<()> {
    // 尝试读取现有配置，加入防损坏保护
    let initial_config = match fs::read_to_string(CONFIG_FILE) {
        Ok(data) => match toml::from_str(&data) {
            Ok(cfg) => cfg,
            Err(e) => {
                // 如果解析失败（格式写错），备份原文件，防止被空列表强行覆盖！
                let backup_name = format!("{}.broken", CONFIG_FILE);
                let _ = fs::rename(CONFIG_FILE, &backup_name);
                eprintln!("⚠️ 配置文件损坏，已备份为 {}。错误: {}", backup_name, e);
                crate::types::AppConfig::default()
            }
        },
        Err(_) => crate::types::AppConfig::default(),
    };

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
            let (watcher_tx, watcher_rx) = std::sync::mpsc::channel();
            let ctx_clone = cc.egui_ctx.clone();
            file_watcher::start_watching(CONFIG_FILE, watcher_tx, move || {
                ctx_clone.request_repaint();
            });
            Ok(Box::new(gui::FolderIconApp::new(cc, initial_config, watcher_rx)))
        }),
    )
}
