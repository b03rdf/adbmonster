use crate::adb::manager;
use crate::adb::process::prepare_command;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct RecordState {
    pub process: Mutex<Option<tokio::process::Child>>,
    pub remote_path: Mutex<Option<String>>,
}

#[tauri::command]
pub async fn start_record(device_id: String, app: AppHandle) -> Result<String, String> {
    {
        let state = app.state::<RecordState>();
        let mut guard = state.process.lock().map_err(|e| e.to_string())?;
        if let Some(child) = guard.as_mut() {
            if child.try_wait().map_err(|e| e.to_string())?.is_none() {
                return Err("A screen recording is already in progress".to_string());
            }
            guard.take();
        }
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let remote_path = format!("/sdcard/adb_monster_rec_{}.mp4", timestamp);

    let adb_path = manager::find_adb().map_err(|e| e.message)?;
    let child = prepare_command(&adb_path)
        .args([
            "-s",
            &device_id,
            "shell",
            "screenrecord",
            "--time-limit",
            "180",
            &remote_path,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;

    let state = app.state::<RecordState>();
    {
        let mut proc_guard = state.process.lock().map_err(|e| e.to_string())?;
        *proc_guard = Some(child);
    }
    {
        let mut path_guard = state.remote_path.lock().map_err(|e| e.to_string())?;
        *path_guard = Some(remote_path.clone());
    }

    Ok(remote_path)
}

#[tauri::command]
pub async fn stop_record(
    device_id: String,
    local_path: String,
    app: AppHandle,
) -> Result<String, String> {
    let state = app.state::<RecordState>();

    let (proc, remote) = {
        let mut proc_guard = state.process.lock().map_err(|e| e.to_string())?;
        let proc = proc_guard.take();
        let mut path_guard = state.remote_path.lock().map_err(|e| e.to_string())?;
        let remote = path_guard.take();
        (proc, remote)
    };

    let remote_path = remote.ok_or_else(|| "No recording in progress".to_string())?;

    if let Some(mut child) = proc {
        let _ = child.kill().await;
        let _ = child.wait().await;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    manager::pull_file(&device_id, &remote_path, &local_path)
        .await
        .map_err(|e| e.message)?;

    let _ = crate::adb::process::run_shell_raw(&device_id, &format!("rm -f {}", remote_path)).await;

    Ok(local_path)
}

#[tauri::command]
pub async fn is_recording(app: AppHandle) -> Result<bool, String> {
    let state = app.state::<RecordState>();
    let mut guard = state.process.lock().map_err(|e| e.to_string())?;
    let Some(child) = guard.as_mut() else {
        return Ok(false);
    };

    match child.try_wait().map_err(|e| e.to_string())? {
        Some(_) => {
            guard.take();
            Ok(false)
        }
        None => Ok(true),
    }
}

#[tauri::command]
pub async fn take_screenshot(device_id: String, output_path: String) -> Result<String, String> {
    manager::save_screenshot_to_file(&device_id, &output_path)
        .await
        .map_err(|e| e.message)
}

#[tauri::command]
pub async fn pull_file(device_id: String, remote: String, local: String) -> Result<String, String> {
    manager::pull_file(&device_id, &remote, &local)
        .await
        .map_err(|e| e.message)
}

#[tauri::command]
pub async fn pull_clog(
    device_id: String,
    package_name: String,
    local_dir: String,
) -> Result<String, String> {
    manager::pull_clog(&device_id, &package_name, &local_dir)
        .await
        .map_err(|e| e.message)
}
