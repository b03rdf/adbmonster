use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio::process::Child;
use tokio::io::{AsyncBufReadExt, BufReader};

    use crate::adb::manager;
use crate::adb::process::prepare_command;

pub struct LogcatState {
    pub process: Mutex<Option<Child>>,
}

#[tauri::command]
pub async fn start_logcat(
    device_id: String,
    filter: Option<String>,
    app: AppHandle,
) -> Result<String, String> {
    let adb_path = manager::find_adb().map_err(|e| e.message)?;

    let _clear = prepare_command(&adb_path)
        .args(["-s", &device_id, "logcat", "-c"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    if let Ok(mut clear_child) = _clear {
        let _ = clear_child.wait().await;
    }

    let mut args: Vec<&str> = vec!["-s", &device_id, "logcat", "-v", "brief"];
    if let Some(ref f) = filter {
        if !f.is_empty() {
            args.push("-s");
            args.push(f);
        }
    }

    let mut child = prepare_command(&adb_path)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;

    let stdout = child.stdout.take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;

    let app_handle = app.clone();
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    tokio::spawn(async move {
        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }
            let _ = app_handle.emit("logcat:line", line);
        }
        let _ = app_handle.emit("logcat:stopped", "process ended");
    });

    let state = app.state::<LogcatState>();
    {
        let mut process_guard = state.process.lock().map_err(|e| e.to_string())?;
        *process_guard = Some(child);
    }

    Ok("started".to_string())
}

#[tauri::command]
pub async fn stop_logcat(app: AppHandle) -> Result<(), String> {
    let state = app.state::<LogcatState>();
    let child_to_kill = {
        let mut process_guard = state.process.lock().map_err(|e| e.to_string())?;
        process_guard.take()
    };

    if let Some(mut child) = child_to_kill {
        child.kill().await.map_err(|e| e.to_string())?;
        let _ = child.wait().await;
    }

    Ok(())
}

#[tauri::command]
pub async fn is_logcat_running(app: AppHandle) -> Result<bool, String> {
    let state = app.state::<LogcatState>();
    let process_guard = state.process.lock().map_err(|e| e.to_string())?;
    Ok(process_guard.is_some())
}
