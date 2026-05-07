// src/app_state.rs

use std::{collections::HashMap, sync::mpsc, thread, time::Instant};

use crate::{types::AppConfig, utils::{apply_folder_icon, restore_folder_icon}};

#[derive(Clone, Copy)]
pub enum Action {
    Apply,
    Restore,
}


pub struct TaskResult {
    pub folder: String,
    pub action: Action,
    pub success: bool,
    pub msg: Option<String>,
}

pub struct AppState {
    pub config: AppConfig,
    pub status_msg: String,
    pub last_internal_update: Instant, 
    io_tx: mpsc::Sender<TaskResult>,
    io_rx: mpsc::Receiver<TaskResult>,
    watcher_rx: mpsc::Receiver<Result<AppConfig, String>>,
}

impl AppState {
    pub fn new(
        config: AppConfig, 
        watcher_rx: mpsc::Receiver<Result<AppConfig, String>>
    ) -> Self {
        let (io_tx, io_rx) = mpsc::channel();
        Self { 
            config, 
            status_msg: "就绪".into(),
            last_internal_update: Instant::now() - std::time::Duration::from_secs(10),
            io_tx, 
            io_rx, 
            watcher_rx,
        }
    }

    /// 标记发生了内部 UI 修改
    pub fn mark_internal_change(&mut self) {
        self.last_internal_update = Instant::now();
    }

    /// 返回 bool 表示是否需要触发磁盘持久化
    pub fn tick(
        &mut self,
        waker: impl Fn() + Send + Clone + 'static,
    ) -> bool {
        let mut needs_save = false;

        // 1. 处理底层 OS 操作结果
        while let Ok(res) = self.io_rx.try_recv() {
            if res.success {
                self.status_msg = match res.action {
                    Action::Apply => format!("操作成功: {}", res.folder),
                    Action::Restore => format!("已恢复默认: {}", res.folder),
                };
            } else {
                // 操作失败！显示错误信息
                self.status_msg = res.msg.unwrap_or_else(|| "未知错误".into());
                
                // 【核心：乐观更新回滚机制】
                // 在列表中找到那个应用失败的文件夹
                if let Some(mapping) = self.config.mappings.iter_mut()
                    .find(|m| m.folder_path.eq_ignore_ascii_case(&res.folder)) 
                {
                    // 把它从“错误期望的乐观状态”拨乱反正
                    mapping.icon_state = match res.action {
                        Action::Apply => false,  // 申请图标失败了，按钮切回 false (重新应用)
                        Action::Restore => true, // 恢复默认失败了，按钮切回 true (恢复默认)
                    };
                    needs_save = true; // 状态被回滚了，顺便把回滚后的配置落盘
                }
            }
        }

        // 2. 处理热更新及配置文件错误
        while let Ok(watcher_res) = self.watcher_rx.try_recv() {
            match watcher_res {
                Ok(new_config) => {
                    if self.last_internal_update.elapsed().as_secs() < 2 { continue; }

                    if self.config != new_config {
                        self.reconcile_os_state(&self.config, &new_config, waker.clone());
                        self.config = new_config;
                        self.status_msg = "配置已从外部修改，已自动同步系统图标。".into();
                        // 外部加载进来的新配置不需要再 needs_save 写回去了
                    }
                }
                Err(err_msg) => {
                    // 截获配置错误信息并展示在 UI 状态栏上！
                    self.status_msg = format!("⚠️ 配置文件格式错误，热更新失败: {}", err_msg);
                }
            }
        }

        needs_save
    }

    /// 发起非阻塞的后台执行任务
    pub fn spawn_io_task(
        &self, 
        action: Action, 
        folder: String, 
        exe: String,
        waker: impl Fn() + Send + 'static,
    ) {
        let tx = self.io_tx.clone();
        thread::spawn(move || {
            let result = match action {
                Action::Apply => apply_folder_icon(&folder, &exe),
                Action::Restore => restore_folder_icon(&folder),
            };
            let _ = tx.send(TaskResult { folder, action, success: result.is_ok(), msg: result.err() });
            waker(); 
        });
    }
    
    /// 核心逻辑：比对新老配置，将修改自动同步到操作系统
    fn reconcile_os_state(
        &self, 
        old_cfg: &AppConfig, 
        new_cfg: &AppConfig,
        waker: impl Fn() + Send + Clone + 'static,
    ) {
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
                            self.spawn_io_task(Action::Apply, new_m.folder_path.clone(), new_m.exe_path.clone(), waker.clone());
                        } else {
                            self.spawn_io_task(Action::Restore, new_m.folder_path.clone(), new_m.exe_path.clone(), waker.clone());
                        }
                    }
                }
                None => {
                    // 以前没有这个文件夹，现在手动添加了
                    if new_m.icon_state {
                        self.spawn_io_task(Action::Apply, new_m.folder_path.clone(), new_m.exe_path.clone(), waker.clone());
                    }
                }
            }
        }

        // 第二步：检查【被删除】的条目
        for (folder_lower, old_m) in &old_map {
            if !new_map.contains_key(folder_lower) {
                // 如果用户在 config.toml 里直接把这一行删掉了，且之前它是生效的，就把它恢复默认
                if old_m.icon_state {
                    self.spawn_io_task(Action::Restore, old_m.folder_path.clone(), old_m.exe_path.clone(), waker.clone());
                }
            }
        }
    }
}