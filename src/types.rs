// src/types.rs

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct AppConfig {
    pub mappings: Vec<FolderExeMapping>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct FolderExeMapping {
    pub folder_path: String,
    pub exe_path: String,
    pub icon_state: bool,
}
