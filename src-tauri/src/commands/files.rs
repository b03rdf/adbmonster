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
    validate_remote_path(&remote_path, true)?;
    process::run_shell_raw(&device_id, &format!("rm -rf {}", shlex_quote(&remote_path)))
        .await
        .map_err(|e| e.message)?;
    Ok(remote_path)
}

#[tauri::command]
pub async fn create_remote_directory(
    device_id: String,
    remote_path: String,
) -> Result<String, String> {
    validate_remote_path(&remote_path, false)?;
    process::run_shell_raw(
        &device_id,
        &format!("mkdir -p {}", shlex_quote(&remote_path)),
    )
    .await
    .map_err(|e| e.message)?;
    Ok(remote_path)
}

fn validate_remote_path(path: &str, destructive: bool) -> Result<(), String> {
    if path.is_empty() || !path.starts_with('/') || path.contains('\0') {
        return Err("Remote path must be a non-empty absolute path".to_string());
    }

    if path.split('/').any(|segment| matches!(segment, "." | "..")) {
        return Err("Remote path must not contain '.' or '..' segments".to_string());
    }

    let normalized = path.trim_end_matches('/');
    if destructive
        && matches!(
            normalized,
            "" | "/sdcard"
                | "/storage"
                | "/data"
                | "/system"
                | "/vendor"
                | "/product"
                | "/proc"
                | "/sys"
                | "/dev"
        )
    {
        return Err(format!("Refusing to delete protected path: {path}"));
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{shlex_quote, validate_remote_path};

    #[test]
    fn quotes_shell_paths() {
        assert_eq!(shlex_quote("/sdcard/a b.txt"), "'/sdcard/a b.txt'");
        assert_eq!(shlex_quote("/sdcard/a'b.txt"), "'/sdcard/a'\"'\"'b.txt'");
    }

    #[test]
    fn rejects_dangerous_delete_paths() {
        for path in ["", "/", "/sdcard", "/sdcard/", "/storage", "/data"] {
            assert!(validate_remote_path(path, true).is_err(), "{path}");
        }
        assert!(validate_remote_path("relative/path", true).is_err());
        assert!(validate_remote_path("/sdcard/..", true).is_err());
        assert!(validate_remote_path("/sdcard/./Download", true).is_err());
        assert!(validate_remote_path("/sdcard/Download/file.txt", true).is_ok());
    }
}
