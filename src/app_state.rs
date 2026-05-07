// src/app_state.rs

use std::{sync::mpsc, thread};

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

        // 2. 处理文件热更新
        while let Ok(new_config) = self.watcher_rx.try_recv() {
            if self.config != new_config {
                self.config = new_config;
                self.status_msg = "配置已自动热重载！".into();
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
}