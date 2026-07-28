use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;

use crate::adb::manager;
use crate::adb::process::prepare_command;

pub struct LogcatState {
    pub process: Mutex<Option<Child>>,
}

#[tauri::command]
pub async fn start_logcat(
    device_id: String,
    buffer: Option<String>,
    app: AppHandle,
) -> Result<String, String> {
    let adb_path = manager::find_adb().map_err(|e| e.message)?;

    let previous = {
        let state = app.state::<LogcatState>();
        let mut guard = state.process.lock().map_err(|e| e.to_string())?;
        guard.take()
    };
    if let Some(mut child) = previous {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    let selected_buffer = match buffer.as_deref().unwrap_or("main") {
        "main" | "system" | "events" | "radio" | "crash" | "all" => {
            buffer.unwrap_or_else(|| "main".to_string())
        }
        value => return Err(format!("Unsupported logcat buffer: {value}")),
    };
    let mut args: Vec<&str> = vec!["-s", &device_id, "logcat", "-v", "threadtime"];
    if selected_buffer != "main" {
        args.push("-b");
        args.push(&selected_buffer);
    }

    let mut child = prepare_command(&adb_path)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;
    let stderr = child.stderr.take();

    let app_handle = app.clone();
    let mut lines = BufReader::new(stdout).lines();

    tokio::spawn(async move {
        let mut batch = Vec::with_capacity(128);
        let mut flush = tokio::time::interval(std::time::Duration::from_millis(50));
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) if !line.is_empty() => batch.push(line),
                        Ok(Some(_)) => {}
                        Ok(None) | Err(_) => break,
                    }
                }
                _ = flush.tick() => {
                    if !batch.is_empty() {
                        let _ = app_handle.emit("logcat:batch", std::mem::take(&mut batch));
                    }
                }
            }
        }
        if !batch.is_empty() {
            let _ = app_handle.emit("logcat:batch", batch);
        }
        let _ = app_handle.emit("logcat:stopped", "process ended");
    });

    if let Some(stderr) = stderr {
        let app_handle = app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    let _ = app_handle.emit("logcat:error", line);
                }
            }
        });
    }

    let state = app.state::<LogcatState>();
    {
        let mut process_guard = state.process.lock().map_err(|e| e.to_string())?;
        *process_guard = Some(child);
    }

    Ok("started".to_string())
}

#[tauri::command]
pub async fn export_logcat(lines: Vec<String>, output_path: String) -> Result<String, String> {
    if output_path.trim().is_empty() {
        return Err("Output path must not be empty".to_string());
    }
    if lines.len() > 50_000 {
        return Err("Too many log lines to export".to_string());
    }
    let mut contents = lines.join("\n");
    if !contents.is_empty() {
        contents.push('\n');
    }
    tokio::fs::write(&output_path, contents)
        .await
        .map_err(|error| error.to_string())?;
    Ok(output_path)
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
