// src/gui.rs

use eframe::egui;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::Arc;

use crate::app_state::AppState;
use crate::config_store::ConfigStore;
use crate::icon_provider::IconProvider;
use crate::app_state::Action;
use crate::types::FolderExeMapping;
use crate::CONFIG_FILE;

// --- UI 应用程序 ---

enum PendingOp {
    ApplyRow(usize),
    RestoreRow(usize),
    RemoveRow(usize),
}

pub struct FolderIconApp {
    state: AppState,
    store: ConfigStore,
    provider: IconProvider,
    
    new_folder: String,
    new_exe: String,

    texture_cache: RefCell<HashMap<String, Option<egui::TextureHandle>>>,
    default_icon: OnceLock<egui::TextureHandle>,
}

impl FolderIconApp {
    /// 构造函数，在创建 App 实例时调用
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_config: crate::types::AppConfig,
        watcher_rx: std::sync::mpsc::Receiver<Result<crate::types::AppConfig, String>>
    ) -> Self {
        Self::setup_custom_fonts(&cc.egui_ctx);
        
        Self {
            state: AppState::new(initial_config, watcher_rx),
            store: ConfigStore::new(CONFIG_FILE),
            provider: IconProvider::new(),
            texture_cache: RefCell::new(HashMap::new()),
            new_folder: String::new(),
            new_exe: String::new(),
            default_icon: OnceLock::new(),
        }
    }

    /// UI 层自己负责“获取/触发加载”纹理
    fn get_texture(&self, ctx: &egui::Context, exe_path: &str) -> Option<egui::TextureHandle> {
        if exe_path.is_empty() { return None; }

        let mut cache = self.texture_cache.borrow_mut();
        
        if !cache.contains_key(exe_path) {
            // 1. 在 UI 缓存中标记为 Loading (None)
            cache.insert(exe_path.to_string(), None);
            
            // 2. 将 ctx 闭包作为 Waker 传给底层逻辑
            let ctx_clone = ctx.clone();
            self.provider.fetch_icon_async(exe_path.to_string(), move || {
                ctx_clone.request_repaint(); // 专门给 egui 用的唤醒逻辑
            });
            return None;
        }

        cache.get(exe_path).unwrap().clone()
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
}

impl eframe::App for FolderIconApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame)  {
        // 1. 核心：每帧无条件调用，消化后台的成功/报错消息和文件热更新！
        let tick_waker_ctx = ui.ctx().clone();
        if self.state.tick(move || tick_waker_ctx.request_repaint()) {
            self.store.save_debounced(self.state.config.clone());
        }
        // 2. 接收底层 IconProvider 发来的像素数据，转换为 GPU 纹理
        while let Ok((path, raw_opt)) = self.provider.rx.try_recv() {
            let tex_opt = raw_opt.map(|raw| {
                let img = egui::ColorImage::from_rgba_unmultiplied([raw.width as usize, raw.height as usize], 
                    &raw.pixels
                );
                ui.ctx().load_texture(&path, img, egui::TextureOptions::LINEAR)
            });
            // 存入 UI 层的字典中
            self.texture_cache.borrow_mut().insert(path, tex_opt);
        }
        // 3. 初始化默认的文件夹图标
        self.init_default_icon(ui.ctx());

        // --- 1. 优先绘制底部状态栏 ---
        // TopBottomPanel::bottom 会将这部分永远固定在窗口最底部
        egui::Panel::bottom("status_panel").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.label(format!("状态: {}", self.state.status_msg));
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
                                            let ctx_clone = ui.ctx().clone();
                                            let waker = move || ctx_clone.request_repaint();
                                            // 乐观更新：直接改内存
                                            self.state.config.mappings.push(FolderExeMapping {
                                                folder_path: self.new_folder.clone(),
                                                exe_path: self.new_exe.clone(),
                                                icon_state: true,
                                            });
                                            // 触发防抖写盘和系统IO
                                            self.state.mark_internal_change();
                                            self.store.save_debounced(self.state.config.clone());
                                            self.state.spawn_io_task(Action::Apply, self.new_folder.clone(), self.new_exe.clone(), waker);
                                            
                                            self.new_folder.clear();
                                            self.new_exe.clear();
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
                                            self.state.status_msg = "已经清空输入框".to_string();
                                            self.new_folder.clear();
                                            self.new_exe.clear();
                                        } else {
                                            self.state.status_msg = "输入框是空的，不需要清空".to_string();
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
                            let preview_tex = self.get_texture(ui.ctx(), &current_exe);
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
                for (idx, mapping) in self.state.config.mappings.iter().enumerate() {
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
                                            pending_op = Some(if applied { 
                                                PendingOp::RestoreRow(idx)  
                                            } else { 
                                                PendingOp::ApplyRow(idx) 
                                            });
                                        }
                                        if ui.button("🗑 移除").clicked() {
                                            pending_op = Some(PendingOp::RemoveRow(idx));
                                        }
                                    });
                                }
                            );

                            // --- 列表右侧：显示应用图标 ---
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let tex = if applied {
                                    // 这里使用 &self 借用，完全合法
                                    self.get_texture(ui.ctx(), &mapping.exe_path)
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
                let ctx_clone = ui.ctx().clone();
                let waker = move || ctx_clone.request_repaint();
                match op {
                    PendingOp::ApplyRow(idx) => {
                        self.state.config.mappings[idx].icon_state = true;
                        let m = &self.state.config.mappings[idx];
                        self.state.spawn_io_task(Action::Apply, m.folder_path.clone(), m.exe_path.clone(), waker);
                    }
                    PendingOp::RestoreRow(idx) => {
                        self.state.config.mappings[idx].icon_state = false;
                        let m = &self.state.config.mappings[idx];
                        self.state.spawn_io_task(Action::Restore, m.folder_path.clone(), m.exe_path.clone(), waker);
                    }
                    PendingOp::RemoveRow(idx) => {
                        let m = self.state.config.mappings.remove(idx);
                        self.state.spawn_io_task(Action::Restore, m.folder_path, m.exe_path, waker);
                    }
                }
                // 标记内部发生修改（屏蔽接下来的文件监听 2 秒钟）
                self.state.mark_internal_change();
                // 派发持久化写盘
                self.store.save_debounced(self.state.config.clone());
            }
        });
    }
}