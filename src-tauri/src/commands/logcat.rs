use crate::adb::{manager, process::prepare_command};
use crate::tasks::{self, TaskKind, TaskManager};
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct LogcatState {
    pub operation: tokio::sync::Mutex<()>,
}

#[tauri::command]
pub async fn start_logcat(
    device_id: String,
    buffer: Option<String>,
    app: AppHandle,
) -> Result<String, String> {
    let selected_buffer = buffer.unwrap_or_else(|| "main".to_string());
    if !matches!(
        selected_buffer.as_str(),
        "main" | "system" | "events" | "radio" | "crash" | "all"
    ) {
        return Err(format!("Unsupported logcat buffer: {selected_buffer}"));
    }
    let state = app.state::<LogcatState>();
    let _operation = state.operation.lock().await;
    tasks::stop_kind(&app, TaskKind::Logcat).await?;
    let task = tasks::begin(&app, TaskKind::Logcat, &device_id, None)?;
    let result = (|| {
        let adb_path = manager::find_adb().map_err(|error| error.message)?;
        prepare_command(&adb_path)
            .args([
                "-s",
                &device_id,
                "logcat",
                "-v",
                "threadtime",
                "-b",
                &selected_buffer,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| error.to_string())
    })();
    match result {
        Ok(child) => {
            let id = task.id.clone();
            tasks::supervise_process(task, child, device_id, true);
            Ok(id)
        }
        Err(error) => {
            task.finish_result::<()>(&Err(error.clone()), "未创建运行中的子进程");
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn stop_logcat(app: AppHandle) -> Result<(), String> {
    let state = app.state::<LogcatState>();
    let _operation = state.operation.lock().await;
    tasks::stop_kind(&app, TaskKind::Logcat).await
}

#[tauri::command]
pub async fn is_logcat_running(app: AppHandle) -> Result<bool, String> {
    Ok(app
        .state::<TaskManager>()
        .current(TaskKind::Logcat)
        .is_some())
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
