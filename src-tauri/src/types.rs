use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub model: String,
    pub android_version: String,
    pub status: String,
    pub connection_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoConnectResult {
    pub connected: bool,
    pub address: String,
    pub message: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct AppConfig {
    pub adb_path: String,
    pub screenshot_output: String,
    pub clog_output: String,
    pub default_package: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFile {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMetrics {
    pub battery_level: Option<u8>,
    pub battery_temperature_c: Option<f32>,
    pub charging: bool,
    pub memory_total_kb: u64,
    pub memory_available_kb: u64,
    pub storage_total_kb: u64,
    pub storage_available_kb: u64,
    pub cpu_usage_percent: Option<f32>,
    pub uptime_seconds: u64,
    pub foreground_activity: String,
    pub collected_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticProgress {
    pub stage: String,
    pub message: String,
    pub percent: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticStatus {
    pub running: bool,
    pub progress: Option<DiagnosticProgress>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeakNetworkCapabilities {
    pub mode: String,
    pub supported: bool,
    pub interface_name: Option<String>,
    pub supports_bandwidth: bool,
    pub supports_downlink: bool,
    pub supports_packet_effects: bool,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeakNetworkConfig {
    pub upload_kbps: u32,
    pub download_kbps: u32,
    pub latency_ms: u32,
    pub jitter_ms: u32,
    pub loss_percent: f32,
    pub duplicate_percent: f32,
    pub reorder_percent: f32,
    pub duration_seconds: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeakNetworkStatus {
    pub active: bool,
    pub device_id: Option<String>,
    pub mode: Option<String>,
    pub interface_name: Option<String>,
    pub expires_at: Option<String>,
    pub message: String,
}
