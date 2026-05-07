// src/icon_cache.rs

use std::{collections::HashMap, sync::mpsc::{self, Receiver, Sender}, thread};

use eframe::egui;

use crate::icon_extractor;

enum IconState {
    Loading,
    Ready(Option<egui::TextureHandle>),
}

type IconMsg = (String, Option<egui::ColorImage>);

pub struct IconCache {
    cache: HashMap<String, IconState>,
    tx: Sender<IconMsg>,
    rx: Receiver<IconMsg>,
}

impl IconCache {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { cache: HashMap::new(), tx, rx }
    }

    /// 在每帧顶部调用，处理后台解析好的图标
    pub fn tick(&mut self, ctx: &egui::Context) {
        while let Ok((path, img_opt)) = self.rx.try_recv() {
            let tex_opt = img_opt.map(|img| {
                ctx.load_texture(&path, img, egui::TextureOptions::LINEAR)
            });
            self.cache.insert(path, IconState::Ready(tex_opt));
        }
    }

    /// 请求图标缓存（不会阻塞）
    pub fn get(&mut self, ctx: &egui::Context, exe_path: &str) -> Option<egui::TextureHandle> {
        if exe_path.is_empty() { return None; }

        if !self.cache.contains_key(exe_path) {
            self.cache.insert(exe_path.to_string(), IconState::Loading);
            
            let tx = self.tx.clone();
            let path = exe_path.to_string();
            let ctx_clone = ctx.clone();
            
            thread::spawn(move || {
                let img_opt = icon_extractor::get_exe_icon_pixels(&path).map(|(pixels, w, h)| {
                    egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels)
                });
                let _ = tx.send((path, img_opt));
                ctx_clone.request_repaint(); // 唤醒 UI 重新绘制
            });
            return None;
        }

        match self.cache.get(exe_path).unwrap() {
            IconState::Loading => None,
            IconState::Ready(tex_opt) => tex_opt.clone(),
        }
    }
}