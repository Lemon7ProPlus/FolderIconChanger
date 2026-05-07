// src/utiles.rs

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::{fs, thread};
use std::time::Duration;

pub const CREATE_NO_WINDOW: u32 = 0x08000000;

// --- 核心业务逻辑 ---

/// 为文件夹设置图标
pub fn apply_folder_icon(folder: &str, exe: &str) -> Result<(), String> {
    let desktop_ini = format!("{}\\desktop.ini", folder);
    let ini_path = Path::new(&desktop_ini);

    // 1. 彻底去除属性：不仅是隐藏(-h)和系统(-s)，必须加上去除只读(-r)
    if ini_path.exists() {
        let _ = Command::new("attrib")
            .args(["-h", "-s", "-r", &desktop_ini])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        // 安全起见，直接删掉旧文件，而不是覆盖写入，避免残留的文件权限导致死锁
        let _ = fs::remove_file(&desktop_ini);
    }
    
    let content = format!(
        "[.ShellClassInfo]\r\nIconResource={},0\r\n",
        exe
    );
    // 2. 写入文件（加入防“资源管理器锁”重试机制）
    let mut write_result = fs::write(&desktop_ini, &content);
    if write_result.is_err() {
        // 如果遭遇 os error 5 (通常是 explorer.exe 正在读取)，短暂停顿后重试3次
        for _ in 0..3 {
            thread::sleep(Duration::from_millis(100));
            write_result = fs::write(&desktop_ini, &content);
            if write_result.is_ok() {
                break;
            }
        }
    }
    // 如果重试依然失败，则抛出详细错误
    write_result.map_err(|e| format!("写入 desktop.ini 失败: {}", e))?;

    // 3. 将 desktop.ini 设置为隐藏(h)和系统(s)
    Command::new("attrib")
        .args(["+h", "+s", &desktop_ini])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("设置文件属性失败: {}", e))?;

    // 4. 将文件夹本身设置为只读(r)，这是 Windows 识别 desktop.ini 的必要条件
    Command::new("attrib")
        .args(["+r", folder])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("设置文件夹属性失败: {}", e))?;

    Ok(())
}

/// 恢复默认文件夹图标
pub fn restore_folder_icon(folder: &str) -> Result<(), String> {
    let desktop_ini = format!("{}\\desktop.ini", folder);
    let ini_path = Path::new(&desktop_ini);

    if ini_path.exists() {
        // 彻底去除所有保护属性 (-r 关键)
        let _ = Command::new("attrib")
            .args(["-h", "-s", "-r", &desktop_ini])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        // 尝试删除（同样加入重试机制防止被锁）
        let mut remove_result = fs::remove_file(&desktop_ini);
        if remove_result.is_err() {
            for _ in 0..3 {
                thread::sleep(Duration::from_millis(100));
                remove_result = fs::remove_file(&desktop_ini);
                if remove_result.is_ok() { break; }
            }
        }
    }

    // 去除文件夹的只读属性，彻底解除特殊状态
    Command::new("attrib")
        .args(["-r", folder])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("恢复文件夹属性失败: {}", e))?;

    Ok(())
}

