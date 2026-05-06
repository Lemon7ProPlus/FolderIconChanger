// src/gui.rs

use eframe::egui;
use std::sync::Arc;
use std::{collections::HashMap, fs};
use std::sync::OnceLock;
use std::cell::RefCell;
use std::sync::mpsc::{self, Sender, Receiver};
use std::thread;

use crate::{
    constants::CONFIG_FILE, 
    icon_extractor, 
    types::{AppConfig, FolderExeMapping}, 
    utils::{apply_folder_icon, restore_folder_icon}
};

// --- UI 应用程序 ---

#[derive(Clone, Copy)]
enum Action {
    Apply,
    Restore,
}

enum PendingOp {
    ApplyRow(usize),
    RestoreRow(usize),
    RemoveRow(usize),
}

// 新增状态枚举
enum IconState {
    Loading,
    Ready(Option<egui::TextureHandle>),
}

// 定义线程之间传递的消息：(EXE路径, 解析出的像素图)
type IconMsg = (String, Option<egui::ColorImage>);

#[derive(Hash, Eq, PartialEq, Clone)]
struct MappingKey {
    folder_path: String,
    exe_path: String,
}

pub struct FolderIconApp {
    config: AppConfig,
    index: HashMap<MappingKey, usize>,
    new_folder: String,
    new_exe: String,
    status_msg: String,
    default_icon: OnceLock<egui::TextureHandle>,
    icon_cache: RefCell<HashMap<String, IconState>>,
    // 多线程通信管道
    icon_tx: Sender<IconMsg>,
    icon_rx: Receiver<IconMsg>,
}

impl Default for FolderIconApp {
    fn default() -> Self {
        let (icon_tx, icon_rx) = mpsc::channel();
        // 启动时读取配置
        let config = if let Ok(data) = fs::read_to_string(CONFIG_FILE) {
            toml::from_str(&data).unwrap_or_default()
        } else {
            AppConfig::default()
        };

        Self {
            config,
            index: HashMap::new(),
            new_folder: String::new(),
            new_exe: String::new(),
            status_msg: "Ready".to_string(),
            default_icon: OnceLock::new(),
            icon_cache: RefCell::new(HashMap::new()),
            icon_tx,
            icon_rx,
        }
    }
}

impl FolderIconApp {
    /// 构造函数，在创建 App 实例时调用
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (icon_tx, icon_rx) = mpsc::channel();
        // 1. 设置中文字体
        Self::setup_custom_fonts(&cc.egui_ctx);
        // 2. 读取配置文件
        let config = if let Ok(data) = fs::read_to_string(CONFIG_FILE) {
            toml::from_str(&data).unwrap_or_default()
        } else {
            AppConfig::default()
        };
        // 3. 构造 app 实例
        let mut app = Self {
            config,
            index: HashMap::new(),
            new_folder: String::new(),
            new_exe: String::new(),
            status_msg: "就绪".to_string(),
            default_icon: OnceLock::new(),
            icon_cache: RefCell::new(HashMap::new()),
            icon_tx,
            icon_rx,
        };
        // 4. 构建 index
        app.rebuild_index();

        app
    }

    /// 设置窗口图标
    pub fn load_icon() -> egui::IconData {
        let bytes = include_bytes!("../assets/folder.png");
        let image = image::load_from_memory(bytes)
            .expect("Invalid image")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        egui::IconData {
            rgba,
            width,
            height,
        }
    }

    /// 设置中文字体
    fn setup_custom_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();
        let font_candidates = [
            "C:\\Windows\\Fonts\\msyhbd.ttc", // 微软雅黑 粗体
            "C:\\Windows\\Fonts\\msyh.ttc",   // 微软雅黑 常规
            "C:\\Windows\\Fonts\\simhei.ttf", // 黑体
            "C:\\Windows\\Fonts\\simsun.ttc", // 宋体
        ];
        let mut font_data = None;
        for path in font_candidates {
            if let Ok(data) = std::fs::read(path) {
                font_data = Some(data);
                break;
            }
        }

        if let Some(data) = font_data {
            fonts.font_data.insert("sys_font".to_owned(), Arc::new(egui::FontData::from_owned(data)).into());
            fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "sys_font".to_owned());
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().push("sys_font".to_owned());
            ctx.set_fonts(fonts);
        }
    }

    /// 序列化配置
    fn normalize(p: &str) -> String {
        p.replace('\\', "/").to_lowercase()
    }
    fn rebuild_index(&mut self) {
        self.index.clear();
        for (i, m) in self.config.mappings.iter().enumerate() {
            let key = MappingKey {
                folder_path: Self::normalize(&m.folder_path),
                exe_path: Self::normalize(&m.exe_path),
            };
            self.index.insert(key, i);
        }
    }

    /// 添加映射
    fn add_mapping(&mut self, folder: String, exe: String) {
        let key = MappingKey {
            folder_path: Self::normalize(&folder),
            exe_path: Self::normalize(&exe),
        };

        // 去重
        if self.index.contains_key(&key) {
            self.status_msg = "已存在该映射".to_string();
            return;
        }

        // 写入 Vec （持久层）
        self.config.mappings.push(FolderExeMapping { 
            folder_path: folder.clone(), 
            exe_path: exe.clone(), 
            icon_state: true 
        });

        // 更新 index （runtime 层）
        self.rebuild_index();

        // 保存配置
        self.save_config();

        // 应用 icon
        match apply_folder_icon(&folder, &exe) {
            Ok(_) => {
                self.status_msg = "添加并应用成功！（可能需要刷新资源管理器以生效）".to_string();
            }
            Err(e) => {
                self.status_msg = e;
            } 
        }
    }

    /// 移除映射
    fn remove_mappling(&mut self, folder: &str, exe: &str) {
        let key = MappingKey {
            folder_path: Self::normalize(folder),
            exe_path: Self::normalize(exe),
        };
        if let Some(&idx) = self.index.get(&key) {
            self.config.mappings.remove(idx);

            self.rebuild_index();
            self.save_config();
            let _ = restore_folder_icon(folder);
            self.status_msg = "已移除记录，并恢复默认！".to_string();
        }
    }

    /// 保存配置到 TOML 文件
    fn save_config(&self) {
        if let Ok(toml_str) = toml::to_string(&self.config) {
            let _ = fs::write(CONFIG_FILE, toml_str);
        }
    }

    /// 设置默认显示图标
    fn init_default_icon(&self, ctx: &egui::Context) {
        self.default_icon.get_or_init(|| {
            let bytes = include_bytes!("../assets/folder.png");
            let image = image::load_from_memory(bytes)
                .expect("Invalid image")
                .to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            let pixels = image.into_vec();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
            ctx.load_texture(
                "default_icon",
                color_image,
                egui::TextureOptions::default(),
            )
        });
    }

    /// 获取缓存的图标（如果不存在，则即时提取并生成 egui 纹理）
    fn get_cached_icon(&self, ctx: &egui::Context, exe_path: &str) -> Option<egui::TextureHandle> {
        if exe_path.is_empty() { return None; }
        // 核心逻辑：如果在缓存中找不到，就去调用 Windows API 提取
        let mut cache = self.icon_cache.borrow_mut();
        // 1. 如果完全没有记录，说明是第一次请求
        if !cache.contains_key(exe_path) {
            cache.insert(exe_path.to_string(), IconState::Loading);

            let tx = self.icon_tx.clone();
            let path = exe_path.to_string();
            let ctx_clone = ctx.clone();
            // 2. 开启后台线程进行耗时的磁盘 I/O 和图像解析
            thread::spawn(move || {
                let img_opt = icon_extractor::get_exe_icon_pixels(&path).map(|(pixels, w, h)| {
                    egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels)
                });
                let _ = tx.send((path, img_opt));
                let _ = &ctx_clone.request_repaint();
            });
            return None;
        }
        // 3. 检查缓存状态
        match cache.get(exe_path).unwrap() {
            IconState::Loading => None, // 还在后台提取中，继续返回 None
            IconState::Ready(tex_opt) => tex_opt.clone(), // 提取完毕，返回真实贴图
        }
    }

    /// 切换图标动作
    fn execute_icon_action(
        &mut self,
        action: Action,
        folder: &str,
        exe: &str,
    ) -> Result<(), String> {
        let result = match action {
            Action::Apply => apply_folder_icon(folder, exe), 
            Action::Restore => restore_folder_icon(folder), 
        };
        if result.is_ok() {
            self.set_icon_state(
                folder, 
                exe, 
                matches!(action, Action::Apply)
            );
            self.rebuild_index();
            self.save_config();
        };
        result
    }

    /// 设置图标状态
    fn set_icon_state(&mut self, folder: &str, exe: &str, state: bool) {
        let key = MappingKey {
            folder_path: Self::normalize(folder),
            exe_path: Self::normalize(exe),
        };
        if let Some(&idx) = self.index.get(&key) {
            self.config.mappings[idx].icon_state = state;
        }
    }
}

impl eframe::App for FolderIconApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame)  {
        // lazy init textures，使用oncecell惰性初始化
        self.init_default_icon(ui.ctx());
        // try_recv() 是非阻塞的，瞬间就能把收件箱清空
        while let Ok((path, img_opt)) = self.icon_rx.try_recv() {
            // 将像素数据上传到显卡，变成 TextureHandle
            let tex_opt = img_opt.map(|img| {
                ui.ctx().load_texture(&path, img, egui::TextureOptions::LINEAR)
            });
            // 更新缓存，状态从 Loading 变为 Ready
            self.icon_cache.borrow_mut().insert(path, IconState::Ready(tex_opt));
        }
        
        // --- 1. 优先绘制底部状态栏 ---
        // TopBottomPanel::bottom 会将这部分永远固定在窗口最底部
        egui::Panel::bottom("status_panel").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.label(format!("状态: {}", self.status_msg));
            ui.add_space(4.0);
        });
        // --- 2. 绘制上方区域和中间的列表区 ---
        // CentralPanel 会自动占据除了底部状态栏以外的所有剩余空间
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Windows 文件夹图标修改器");
            ui.separator();
            ui.heading("添加新映射：");
            // === 添加新映射区域 ===
            ui.group(|ui| {
                ui.set_min_width(ui.available_width());
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
                                            self.add_mapping(
                                                self.new_folder.clone(), 
                                                self.new_exe.clone(),
                                        );
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
                        // 所选EXE的图标预览
                        egui::Layout::right_to_left(egui::Align::Center), 
                        |ui| {
                            let current_exe = self.new_exe.clone();
                            let preview_tex = self.get_cached_icon(ui.ctx(), &current_exe);
                            let default_tex = self.default_icon.get().unwrap();
                            let tex = preview_tex
                                .as_ref()
                                .unwrap_or(default_tex);
                            ui.add(egui::Image::new(tex).fit_to_exact_size(egui::vec2(32.0, 32.0)));
                        }
                    );
                });
            });

            ui.add_space(10.0);
            ui.separator();

            // === 列表区域 ===
            ui.heading("已配置映射:");
            let mut pending_op = None;
            // 因为放在了 CentralPanel 里，ScrollArea 现在只会延伸到状态栏上方，绝对不会重叠
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                for (idx, mapping) in self.config.mappings.iter().enumerate() {
                    let applied = mapping.icon_state;
                    let label = if applied {"↺ 恢复默认"} else {"▶ 重新应用"};
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
                                        if ui.add_sized(egui::vec2(100.0, 20.0), egui::Button::new(label), ).clicked() {
                                            pending_op = Some(if applied { PendingOp::RestoreRow(idx) } else { PendingOp::ApplyRow(idx) });
                                        }
                                        if ui.button("🗑 移除").clicked() {
                                            pending_op = Some(PendingOp::RemoveRow(idx));
                                            // self.remove_mappling(&mapping.folder_path, &mapping.exe_path);
                                        }
                                    });
                                }
                            );

                            // --- 列表右侧：显示应用图标 ---
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let tex = if applied {
                                    // 这里使用 &self 借用，完全合法
                                    self.get_cached_icon(ui.ctx(), &mapping.exe_path)
                                } else {
                                    None
                                };
                                let display_tex = tex.unwrap_or_else(|| self.default_icon.get().unwrap().clone());
                                ui.add(egui::Image::new(&display_tex).fit_to_exact_size(egui::vec2(32.0, 32.0)));
                            });
                        });
                    });
                }
            });
            if let Some(op) = pending_op {
                match op {
                    PendingOp::ApplyRow(idx) | PendingOp::RestoreRow(idx) => {
                        let folder = self.config.mappings[idx].folder_path.clone();
                        let exe = self.config.mappings[idx].exe_path.clone();
                        let action = if matches!(op, PendingOp::ApplyRow(_)) {Action::Apply} else {Action::Restore};
                        match self.execute_icon_action(action, &folder, &exe) {
                            Ok(_) => {
                                self.status_msg = match action {
                                    Action::Apply => format!("已重新应用：{}", folder),
                                    Action::Restore => format!("已恢复默认：{}", folder),
                                };
                            }
                            Err(e) => self.status_msg = e,
                        }
                    }
                    PendingOp::RemoveRow(idx) => {
                        let folder = self.config.mappings[idx].folder_path.clone();
                        let exe = self.config.mappings[idx].exe_path.clone();
                        self.remove_mappling(&folder, &exe);
                    }
                }
            }
        });
    }
}