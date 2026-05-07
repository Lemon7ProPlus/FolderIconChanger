// src/icon_provider.rs

use std::sync::mpsc::{self, Sender, Receiver};
use std::thread;

use crate::icon_extractor;

/// 纯粹的平台无关图像结构
pub struct RawImage {
    pub pixels: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

pub struct IconProvider {
    tx: Sender<(String, Option<RawImage>)>,
    pub rx: Receiver<(String, Option<RawImage>)>,
}

impl IconProvider {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx }
    }

    /// 发起提取请求，提取完成后调用 waker 通知外层 UI 重绘
    pub fn fetch_icon_async(&self, exe_path: String, waker: impl Fn() + Send + 'static) {
        let tx = self.tx.clone();
        
        thread::spawn(move || {
            // 纯底层的数据提取
            let img_opt = icon_extractor::get_exe_icon_pixels(&exe_path)
                .map(|(pixels, width, height)| RawImage { 
                    pixels, 
                    width: width, 
                    height: height,
                });
                
            let _ = tx.send((exe_path, img_opt));
            
            // 核心解耦点：后台线程不知道是 egui 还是 GPUI，只负责“按铃”
            waker(); 
        });
    }
}