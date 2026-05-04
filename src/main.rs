// src/main.rs

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::windows::process::CommandExt;
use std::process::Command;
mod icon_extractor;

const CONFIG_FILE: &str = "mappings.toml";
const CREATE_NO_WINDOW: u32 = 0x08000000;

// --- 数据结构 ---

#[derive(Serialize, Deserialize, Clone, Default)]
struct AppConfig {
    mappings: Vec<FolderExeMapping>,
}
#[derive(Serialize, Deserialize, Clone)]
struct FolderExeMapping {
    folder_path: String,
    exe_path: String,
}

// --- 核心业务逻辑 ---

/// 为文件夹设置图标
fn apply_folder_icon(folder: &str, exe: &str) -> Result<(), String> {
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
fn restore_folder_icon(folder: &str) -> Result<(), String> {
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

// --- UI 应用程序 ---

struct FolderIconApp {
    config: AppConfig,
    new_folder: String,
    new_exe: String,
    status_msg: String,
    icon_cache: std::collections::HashMap<String, Option<egui::TextureHandle>>,
}

impl Default for FolderIconApp {
    fn default() -> Self {
        // 启动时读取配置
        let config = if let Ok(data) = fs::read_to_string(CONFIG_FILE) {
            toml::from_str(&data).unwrap_or_default()
        } else {
            AppConfig::default()
        };

        Self {
            config,
            new_folder: String::new(),
            new_exe: String::new(),
            status_msg: "Ready".to_string(),
            icon_cache: std::collections::HashMap::new(),
        }
    }
}

impl FolderIconApp {
    /// 构造函数，在创建 App 实例时调用
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 1. 设置中文字体
        Self::setup_custom_fonts(&cc.egui_ctx);

        // 2. 读取配置文件
        let config = if let Ok(data) = fs::read_to_string(CONFIG_FILE) {
            toml::from_str(&data).unwrap_or_default()
        } else {
            AppConfig::default()
        };

        // 3. 返回 App 实例
        Self {
            config,
            new_folder: String::new(),
            new_exe: String::new(),
            status_msg: "就绪".to_string(),
            icon_cache: std::collections::HashMap::new(),
        }
    }

    /// 设置中文字体
    fn setup_custom_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();
        let font_candidates = ["C:\\Windows\\Fonts\\msyhbd.ttc", "C:\\Windows\\Fonts\\msyh.ttc"];
        let mut font_data = None;
        for path in font_candidates {
            if let Ok(data) = std::fs::read(path) {
                font_data = Some(data);
                break;
            }
        }

        if let Some(data) = font_data {
            fonts.font_data.insert("sys_font".to_owned(), egui::FontData::from_owned(data).into());
            fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "sys_font".to_owned());
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().push("sys_font".to_owned());
            ctx.set_fonts(fonts);
        }
    }

    /// 保存配置到 TOML 文件
    fn save_config(&self) {
        if let Ok(toml_str) = toml::to_string(&self.config) {
            let _ = fs::write(CONFIG_FILE, toml_str);
        }
    }

    /// 获取缓存的图标（如果不存在，则即时提取并生成 egui 纹理）
    fn get_cached_icon(&mut self, ctx: &egui::Context, exe_path: &str) -> Option<egui::TextureHandle> {
        if exe_path.is_empty() { return None; }
        
        // 核心逻辑：如果在缓存中找不到，就去调用 Windows API 提取
        if !self.icon_cache.contains_key(exe_path) {
            let tex = if let Some((pixels, w, h)) = icon_extractor::get_exe_icon_pixels(exe_path) {
                // 转换像素数据为 egui 支持的 ColorImage
                let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels);
                Some(ctx.load_texture(exe_path, img, egui::TextureOptions::LINEAR))
            } else {
                None
            };
            self.icon_cache.insert(exe_path.to_string(), tex);
        }
        
        self.icon_cache.get(exe_path).unwrap().clone()
    }
}

impl eframe::App for FolderIconApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame)  {
        // --- 1. 优先绘制底部状态栏 ---
        // TopBottomPanel::bottom 会将这部分永远固定在窗口最底部
        egui::TopBottomPanel::bottom("status_panel").show(ctx, |ui| {
            ui.add_space(4.0); // 留一点上下边距，好看一些
            ui.label(format!("状态: {}", self.status_msg));
            ui.add_space(4.0);
        });

        // --- 2. 绘制上方区域和中间的列表区 ---
        // CentralPanel 会自动占据除了底部状态栏以外的所有剩余空间
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Windows 文件夹图标修改器");
            ui.separator();

            // === 添加新映射区域 ===
            ui.group(|ui| {
                ui.set_min_width(ui.available_width());
                ui.label("添加新规则：");

                ui.horizontal(|ui| {
                    let left_width = ui.available_width() - 42.0;
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_width, ui.available_height()), 
                        egui::Layout::top_down(egui::Align::Min), 
                        |ui| {
                            ui.horizontal(|ui|{
                                ui.label("文件夹儿路径:");
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("➕ 添加").clicked() {
                                        if !self.new_folder.is_empty() && !self.new_exe.is_empty() {
                                            self.config.mappings.push(FolderExeMapping {
                                                folder_path: self.new_folder.clone(),
                                                exe_path: self.new_exe.clone(),
                                            });
                                            
                                            match apply_folder_icon(&self.new_folder, &self.new_exe) {
                                                Ok(_) => self.status_msg = "添加并应用成功！(可能需要刷新资源管理器才能看到变化)".to_string(),
                                                Err(e) => self.status_msg = e,
                                            }
                                            
                                            self.save_config();
                                            self.new_folder.clear();
                                            self.new_exe.clear();
                                        } else {
                                            self.status_msg = "请填写完整的文件夹和EXE路径".to_string();
                                        }
                                    }
                                    if ui.button("浏览...").clicked() {
                                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                            self.new_folder = path.display().to_string();
                                        }
                                    }
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.new_folder)
                                            .desired_width(f32::INFINITY)
                                    );
                                });
                            });

                            ui.horizontal(|ui| {
                                ui.label("应用程序路径:");
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("🔃 清空").clicked() {
                                        if !self.new_folder.is_empty() || !self.new_exe.is_empty() {
                                            self.status_msg = "已经清空输入框".to_string();
                                            self.new_folder.clear();
                                            self.new_exe.clear();
                                        } else {
                                            self.status_msg = "输入框是空的，不需要清空".to_string();
                                        }
                                    }
                                    if ui.button("浏览...").clicked() {
                                        if let Some(path) = rfd::FileDialog::new().add_filter("Exe", &["exe"]).pick_file() {
                                            self.new_exe = path.display().to_string();
                                        }
                                    }
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.new_exe)
                                            .desired_width(f32::INFINITY)
                                    );
                                });
                            });
                        }
                    );
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center), 
                        |ui| {
                            let current_exe = self.new_exe.clone();
                            let preview_tex = self.get_cached_icon(ctx, &current_exe);
    
                            if let Some(tex) = &preview_tex {
                                ui.add(egui::Image::new(tex).fit_to_exact_size(egui::vec2(32.0, 32.0)));
                            } else {
                                ui.add_sized([32.0, 32.0], egui::Label::new("📄"));
                            }

                        }
                    );
                });
            });

            ui.add_space(10.0);
            ui.separator();

            // === 列表区域 ===
            ui.heading("已配置的映射:");
            let mut index_to_remove = None;
            let cloned_mappings = self.config.mappings.clone();

            // 因为放在了 CentralPanel 里，ScrollArea 现在只会延伸到状态栏上方，绝对不会重叠
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                for (i, mapping) in cloned_mappings.into_iter().enumerate() {
                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        
                        ui.horizontal(|ui| {
                            // --- 列表左侧：占据绝大部分空间，放路径和操作按钮 ---
                            let left_width = ui.available_width() - 50.0; // 留出50像素给右边的图标
                            
                            ui.allocate_ui_with_layout(
                                egui::vec2(left_width, ui.available_height()),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    // 路径文本 (截断防撑破)
                                    ui.add(egui::Label::new(format!("📁 文件夹儿: {}", mapping.folder_path)).wrap_mode(egui::TextWrapMode::Truncate))
                                      .on_hover_text(&mapping.folder_path);
                                    ui.add(egui::Label::new(format!("📱 应用程序: {}", mapping.exe_path)).wrap_mode(egui::TextWrapMode::Truncate))
                                      .on_hover_text(&mapping.exe_path);
                                    
                                    ui.add_space(4.0); // 文字和按钮的间距
                                    
                                    // 操作按钮
                                    ui.horizontal(|ui| {
                                        if ui.button("▶ 重新应用").clicked() {
                                            match apply_folder_icon(&mapping.folder_path, &mapping.exe_path) {
                                                Ok(_) => self.status_msg = format!("已重新应用: {}", mapping.folder_path),
                                                Err(e) => self.status_msg = e,
                                            }
                                        }
                                        if ui.button("↺ 恢复默认").clicked() {
                                            match restore_folder_icon(&mapping.folder_path) {
                                                Ok(_) => self.status_msg = format!("已恢复默认: {}", mapping.folder_path),
                                                Err(e) => self.status_msg = e,
                                            }
                                        }
                                        if ui.button("🗑 移除").clicked() {
                                            index_to_remove = Some(i);
                                        }
                                    });
                                }
                            );

                            // --- 列表右侧：显示应用图标 ---
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let list_icon = self.get_cached_icon(ctx, &mapping.exe_path);
                                if let Some(tex) = &list_icon {
                                    ui.add(egui::Image::new(tex).fit_to_exact_size(egui::vec2(32.0, 32.0)));
                                } else {
                                    ui.add_sized([32.0, 32.0], egui::Label::new("📄"));
                                }
                            });
                        });
                    });
                }
            });

            if let Some(i) = index_to_remove {
                self.config.mappings.remove(i);
                self.save_config();
                self.status_msg = "已移除记录".to_string();
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
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
