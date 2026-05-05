// src/main.rs

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use eframe::egui;
use crate::gui::FolderIconApp;

mod icon_extractor;
mod types;
mod utils;
mod constants;
mod gui;

fn main() -> eframe::Result<()> {
    // let icon = load_icon();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(FolderIconApp::load_icon())
            .with_inner_size([500.0, 460.0])
            .with_min_inner_size([400.0, 305.0])
            .with_title("Folder Icon Changer"),
        ..Default::default()
    };
    
    eframe::run_native(
        "Folder Icon Changer",
        options,
        Box::new(|cc| Ok(Box::new(FolderIconApp::new(cc)))),
    )
}
