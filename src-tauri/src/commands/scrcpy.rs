use crate::tasks::{self, TaskKind, TaskManager};
use std::process::Stdio;
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct ScrcpyState {
    pub operation: tokio::sync::Mutex<()>,
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
    let state = app.state::<ScrcpyState>();
    let _operation = state.operation.lock().await;
    let task = tasks::begin(&app, TaskKind::Scrcpy, &device_id, None)?;
    let result = (|| {
        let resource_dir = app.path().resource_dir().ok();
        let path = find_scrcpy(resource_dir.as_deref())?;
        crate::adb::process::prepare_command(&path)
            .args(["-s", &device_id, "--no-audio"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("scrcpy 启动失败：{error}"))
    })();
    match result {
        Ok(child) => {
            tasks::supervise_process(task, child, device_id, false);
            Ok("scrcpy 正在启动，可在任务中心查看结果".to_string())
        }
        Err(error) => {
            task.finish_result::<()>(&Err(error.clone()), "未创建运行中的子进程");
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn stop_scrcpy(app: AppHandle) -> Result<(), String> {
    let state = app.state::<ScrcpyState>();
    let _operation = state.operation.lock().await;
    tasks::stop_kind(&app, TaskKind::Scrcpy).await
}

#[tauri::command]
pub async fn is_scrcpy_running(app: AppHandle) -> Result<bool, String> {
    Ok(app
        .state::<TaskManager>()
        .current(TaskKind::Scrcpy)
        .is_some())
}
