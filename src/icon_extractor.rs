// src/icon_extractor.rs

use std::os::windows::ffi::OsStrExt;
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetIconInfo, HICON, ICONINFO,
};
use windows::Win32::UI::Shell::ExtractIconExW;

/// 提取 EXE 主图标并转换为 (像素数组, 宽, 高)
pub fn get_exe_icon_pixels(exe_path: &str) -> Option<(Vec<u8>, usize, usize)> {
    if exe_path.is_empty() { return None; }
    
    unsafe {
        // 1. 转换路径为宽字符
        let wide_path: Vec<u16> = std::ffi::OsStr::new(exe_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // 2. 提取大图标
        let mut hicon_large = [HICON::default(); 1];
        let extracted = ExtractIconExW(
            PCWSTR(wide_path.as_ptr()), 0, Some(hicon_large.as_mut_ptr()), None, 1,
        );

        if extracted == 0 || hicon_large[0].is_invalid() {
            return None;
        }

        let hicon = hicon_large[0];
        let mut icon_info = ICONINFO::default();
        if GetIconInfo(hicon, &mut icon_info).is_err() {
            let _ = DestroyIcon(hicon);
            return None;
        }

        let hbm_color = icon_info.hbmColor;
        if hbm_color.is_invalid() {
            let _ = DeleteObject(icon_info.hbmMask.into());
            let _ = DestroyIcon(hicon);
            return None; 
        }

        // 3. 获取图标尺寸
        let mut bitmap = BITMAP::default();
        GetObjectW(
            hbm_color.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut _ as *mut _),
        );

        let width = bitmap.bmWidth as usize;
        let height = bitmap.bmHeight as usize;

        // 4. 读取像素数据
        let hdc = CreateCompatibleDC(None);
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), 
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut pixels = vec![0u8; width * height * 4];
        GetDIBits(
            hdc, hbm_color, 0, height as u32, Some(pixels.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS,
        );

        // 5. 释放资源
        let _ = DeleteDC(hdc);
        let _ = DeleteObject(icon_info.hbmColor.into());
        let _ = DeleteObject(icon_info.hbmMask.into());
        let _ = DestroyIcon(hicon);

        // 6. 核心：将 Windows 的 BGRA 转为 egui 的 RGBA
        for chunk in pixels.chunks_exact_mut(4) {
            let b = chunk[0];
            let g = chunk[1];
            let r = chunk[2];
            let a = chunk[3];
            chunk[0] = r;
            chunk[1] = g;
            chunk[2] = b;
            chunk[3] = a;
        }

        Some((pixels, width, height))
    }
}