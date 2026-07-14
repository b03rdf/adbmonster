use std::process::Stdio;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub struct ScrcpyState {
    pub process: Mutex<Option<tokio::process::Child>>,
}

fn find_scrcpy(resource_dir: Option<&std::path::Path>) -> Result<std::path::PathBuf, String> {
    if let Ok(path) = which::which("scrcpy") {
        return Ok(path);
    }

    let candidates: Vec<std::path::PathBuf> = {
        let mut v = Vec::new();
        if let Some(dir) = resource_dir {
            v.push(dir.join("scrcpy").join("scrcpy.exe"));
            v.push(dir.join("scrcpy.exe"));
        }
        v.push(std::path::PathBuf::from("src-tauri/scrcpy/scrcpy.exe"));
        v.push(std::path::PathBuf::from("scrcpy/scrcpy.exe"));
        v.push(std::path::PathBuf::from("scrcpy.exe"));
        if let Ok(exe) = std::env::current_exe() {
            let dir = exe.parent().unwrap_or(&exe);
            v.push(dir.join("scrcpy").join("scrcpy.exe"));
            v.push(dir.join("scrcpy.exe"));
        }
        v
    };

    for c in &candidates {
        if c.exists() {
            return Ok(c.to_path_buf());
        }
    }

    Err("scrcpy not found. Please install scrcpy (https://github.com/Genymobile/scrcpy).\n  Options:\n    - winget install scrcpy\n    - Download zip and extract 'scrcpy/' folder next to this app".to_string())
}

#[tauri::command]
pub async fn start_scrcpy(device_id: String, app: AppHandle) -> Result<String, String> {
    {
        let state = app.state::<ScrcpyState>();
        let mut guard = state.process.lock().map_err(|e| e.to_string())?;
        if let Some(child) = guard.as_mut() {
            if child.try_wait().map_err(|e| e.to_string())?.is_none() {
                return Err("scrcpy 已在运行中".to_string());
            }
            guard.take();
        }
    }

    let resource_dir = app.path().resource_dir().ok();
    let scrcpy_path = find_scrcpy(resource_dir.as_deref())?;

    let mut child = Command::new(&scrcpy_path)
        .args(["-s", &device_id, "--no-audio"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("scrcpy 启动失败: {}", e))?;

    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    if let Ok(Some(status)) = child.try_wait() {
        let stderr_text = if let Some(mut stderr) = child.stderr.take() {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf).await;
            let t = buf.trim().to_string();
            if t.is_empty() {
                String::new()
            } else {
                format!("\n{}", t)
            }
        } else {
            String::new()
        };
        return Err(format!("scrcpy 已退出 (代码: {}){}", status, stderr_text));
    }

    if let Some(mut stderr) = child.stderr.take() {
        let app_handle = app.clone();
        tokio::spawn(async move {
            let mut message = String::new();
            let _ = stderr.read_to_string(&mut message).await;
            if !message.trim().is_empty() {
                let _ = app_handle.emit("scrcpy:error", message);
            }
            let _ = app_handle.emit("scrcpy:stopped", ());
        });
    }

    let state = app.state::<ScrcpyState>();
    {
        let mut guard = state.process.lock().map_err(|e| e.to_string())?;
        *guard = Some(child);
    }

    Ok("scrcpy started".to_string())
}

#[tauri::command]
pub async fn stop_scrcpy(app: AppHandle) -> Result<(), String> {
    let state = app.state::<ScrcpyState>();
    let child_to_kill = {
        let mut guard = state.process.lock().map_err(|e| e.to_string())?;
        guard.take()
    };

    if let Some(mut child) = child_to_kill {
        child.kill().await.map_err(|e| e.to_string())?;
        let _ = child.wait().await;
    }

    Ok(())
}

#[tauri::command]
pub async fn is_scrcpy_running(app: AppHandle) -> Result<bool, String> {
    let state = app.state::<ScrcpyState>();
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
