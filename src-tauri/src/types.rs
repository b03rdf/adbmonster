use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub model: String,
    pub android_version: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct LogEntry {
    pub timestamp: String,
    pub pid: u32,
    pub tid: u32,
    pub level: String,
    pub tag: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct AppConfig {
    pub adb_path: String,
    pub screenshot_output: String,
    pub clog_output: String,
    pub default_package: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            adb_path: String::new(),
            screenshot_output: String::new(),
            clog_output: String::new(),
            default_package: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFile {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}
