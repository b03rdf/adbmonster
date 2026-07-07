use crate::adb::manager;

#[tauri::command]
pub async fn install_apk(device_id: String, apk_path: String) -> Result<String, String> {
    manager::install_apk(&device_id, &apk_path).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn uninstall_apk(device_id: String, package_name: String) -> Result<String, String> {
    manager::uninstall_apk(&device_id, &package_name).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn clear_app(device_id: String, package_name: String) -> Result<String, String> {
    manager::clear_app(&device_id, &package_name).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn get_package_info(device_id: String, package_name: String) -> Result<String, String> {
    manager::get_package_info(&device_id, &package_name).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn pull_apk(device_id: String, package_name: String, local_path: String) -> Result<String, String> {
    manager::pull_apk(&device_id, &package_name, &local_path).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn list_packages(device_id: String) -> Result<Vec<String>, String> {
    manager::list_third_party_packages(&device_id).await.map_err(|e| e.message)
}
