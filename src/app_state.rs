// src/app_state.rs

use std::{collections::HashMap, sync::mpsc, thread};

use crate::{types::AppConfig, utils::{apply_folder_icon, restore_folder_icon}};

#[derive(Clone, Copy)]
pub enum Action {
    Apply,
    Restore,
}


pub struct TaskResult {
    pub folder: String,
    pub exe: String,
    pub action: Action,
    pub success: bool,
    pub msg: Option<String>,
}

pub struct AppState {
    pub config: AppConfig,
    pub status_msg: String,
    io_tx: mpsc::Sender<TaskResult>,
    io_rx: mpsc::Receiver<TaskResult>,
    watcher_rx: mpsc::Receiver<AppConfig>,
}

impl AppState {
    pub fn new(config: AppConfig, watcher_rx: mpsc::Receiver<AppConfig>) -> Self {
        let (io_tx, io_rx) = mpsc::channel();
        Self { config, status_msg: "就绪".into(), io_tx, io_rx, watcher_rx }
    }

    /// 消费并处理所有后台消息
    pub fn tick(&mut self) -> bool {
        let mut config_changed = false;

        // 1. 处理底层 OS 图标操作的结果
        while let Ok(res) = self.io_rx.try_recv() {
            if res.success {
                let state = matches!(res.action, Action::Apply);
                if let Some(mapping) = self.config.mappings.iter_mut()
                    .find(|m| m.folder_path.eq_ignore_ascii_case(&res.folder)) 
                {
                    mapping.icon_state = state;
                    config_changed = true; // 状态变了，需要写盘
                }
                self.status_msg = match res.action {
                    Action::Apply => format!("操作成功: {}", res.folder),
                    Action::Restore => format!("已恢复默认: {}", res.folder),
                };
            } else {
                self.status_msg = res.msg.unwrap_or_else(|| "未知错误".into());
            }
        }

        // 2. 处理文件热更新：加入 Diff 同步逻辑！
        while let Ok(new_config) = self.watcher_rx.try_recv() {
            if self.config != new_config {
                // 只有在真的不一致时，才执行差异调和（防止无限循环）
                self.reconcile_os_state(&self.config, &new_config);
                self.config = new_config;
                self.status_msg = "配置已外部修改，正在自动同步系统图标...".into();
            }
        }

        config_changed
    }

    /// 发起非阻塞的后台执行任务
    pub fn spawn_io_task(&self, action: Action, folder: String, exe: String) {
        let tx = self.io_tx.clone();
        thread::spawn(move || {
            let result = match action {
                Action::Apply => apply_folder_icon(&folder, &exe),
                Action::Restore => restore_folder_icon(&folder),
            };
            let _ = tx.send(TaskResult { folder, exe, action, success: result.is_ok(), msg: result.err() });
        });
    }
    
    /// 核心逻辑：比对新老配置，将修改自动同步到操作系统
    fn reconcile_os_state(&self, old_cfg: &AppConfig, new_cfg: &AppConfig) {
        // 构建旧映射的字典 (忽略路径大小写)
        let mut old_map = HashMap::new();
        for m in &old_cfg.mappings {
            old_map.insert(m.folder_path.to_lowercase(), m);
        }

        let mut new_map = HashMap::new();
        for m in &new_cfg.mappings {
            new_map.insert(m.folder_path.to_lowercase(), m);
        }

        // 第一步：检查【新增】和【被修改】的条目
        for (folder_lower, new_m) in &new_map {
            match old_map.get(folder_lower) {
                Some(old_m) => {
                    // 如果状态变了，或者绑定的 exe 变了
                    if old_m.icon_state != new_m.icon_state || old_m.exe_path != new_m.exe_path {
                        if new_m.icon_state {
                            self.spawn_io_task(Action::Apply, new_m.folder_path.clone(), new_m.exe_path.clone());
                        } else {
                            self.spawn_io_task(Action::Restore, new_m.folder_path.clone(), new_m.exe_path.clone());
                        }
                    }
                }
                None => {
                    // 以前没有这个文件夹，现在手动添加了
                    if new_m.icon_state {
                        self.spawn_io_task(Action::Apply, new_m.folder_path.clone(), new_m.exe_path.clone());
                    }
                }
            }
        }

        // 第二步：检查【被删除】的条目
        for (folder_lower, old_m) in &old_map {
            if !new_map.contains_key(folder_lower) {
                // 如果用户在 config.toml 里直接把这一行删掉了，且之前它是生效的，就把它恢复默认
                if old_m.icon_state {
                    self.spawn_io_task(Action::Restore, old_m.folder_path.clone(), old_m.exe_path.clone());
                }
            }
        }
    }
}