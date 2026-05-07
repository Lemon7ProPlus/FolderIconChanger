// src/types.rs

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct AppConfig {
    pub mappings: Vec<FolderExeMapping>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct FolderExeMapping {
    pub folder_path: String,
    pub exe_path: String,
    pub icon_state: bool,
}