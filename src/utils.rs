// src/utiles.rs

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::fs;

use crate::constants::CREATE_NO_WINDOW;

// --- 核心业务逻辑 ---

/// 为文件夹设置图标
pub fn apply_folder_icon(folder: &str, exe: &str) -> Result<(), String> {
    let desktop_ini = format!("{}\\desktop.ini", folder);
    // 1. 如果存在旧的 desktop.ini，先去除隐藏和系统属性才能覆盖
    let _= Command::new("attrib")
        .args(["-h", "-s", &desktop_ini])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    // 2. 写入 desktop.ini 结构 (索引 0 通常是主图标)
    let content = format!(
        "[.ShellClassInfo]\r\nIconResource={},0\r\n",
        exe
    );
    fs::write(&desktop_ini, content).map_err(|e| format!("写入 desktop.ini失败: {}", e))?;
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

    // 去除文件属性并删除
    let _ = Command::new("attrib")
        .args(["-h", "-s", &desktop_ini])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let _ = fs::remove_file(&desktop_ini);

    // 去除文件夹的只读属性
    Command::new("attrib")
        .args(["-r", folder])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("恢复文件夹属性失败: {}", e))?;

    Ok(())
}

