use crate::adb::manager;
use crate::adb::process;
use crate::types::RemoteFile;

#[tauri::command]
pub async fn list_remote_files(device_id: String, path: String) -> Result<Vec<RemoteFile>, String> {
    manager::list_remote_files(&device_id, &path)
        .await
        .map_err(|e| e.message)
}

#[tauri::command]
pub async fn push_file_to_remote(
    device_id: String,
    local_path: String,
    remote_path: String,
) -> Result<String, String> {
    manager::push_file(&device_id, &local_path, &remote_path)
        .await
        .map_err(|e| e.message)
}

#[tauri::command]
pub async fn delete_remote_file(device_id: String, remote_path: String) -> Result<String, String> {
    process::run_shell_raw(&device_id, &format!("rm -rf {}", shlex_quote(&remote_path)))
        .await
        .map_err(|e| e.message)?;
    Ok(remote_path)
}

#[tauri::command]
pub async fn create_remote_directory(device_id: String, remote_path: String) -> Result<String, String> {
    process::run_shell_raw(&device_id, &format!("mkdir -p {}", shlex_quote(&remote_path)))
        .await
        .map_err(|e| e.message)?;
    Ok(remote_path)
}

fn shlex_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    if s.chars().all(|c| c.is_alphanumeric() || "_./-".contains(c)) {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}
