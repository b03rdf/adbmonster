use crate::adb::manager;
use crate::types::Device;

#[tauri::command]
pub async fn get_devices() -> Result<Vec<Device>, String> {
    manager::list_devices().await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn connect_device(ip: String) -> Result<String, String> {
    manager::connect_device(&ip).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn disconnect_device(ip: String) -> Result<String, String> {
    manager::disconnect_device(&ip).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn get_device_ip(device_id: String) -> Result<String, String> {
    manager::get_device_ip(&device_id).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn tcpip_connect(port: u16) -> Result<String, String> {
    manager::tcpip(port).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn pair_device(ip: String, port: u16, code: String) -> Result<String, String> {
    manager::pair_device(&ip, port, &code).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn pair_then_connect(ip: String, pair_port: u16, connect_port: u16, code: String) -> Result<String, String> {
    let pair_result = manager::pair_device(&ip, pair_port, &code).await.map_err(|e| e.message)?;
    let connect_result = manager::connect_device(&format!("{}:{}", ip, connect_port)).await.map_err(|e| e.message)?;
    Ok(format!("{}; {}", pair_result, connect_result))
}
