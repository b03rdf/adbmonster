use crate::adb::{
    manager,
    process::{prepare_command, run_shell_raw},
};
use crate::tasks::{self, Task, TaskKind, TaskManager, TaskStatus};
use crate::types::RecordingStatus;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tokio::time::{timeout, Duration};

#[derive(Clone)]
pub struct RecordSession {
    device_id: String,
    remote_path: String,
    task: Task,
    running: bool,
    finalized: bool,
}

pub struct RecordState {
    pub operation: tokio::sync::Mutex<()>,
    pub session: Mutex<Option<RecordSession>>,
}

#[tauri::command]
pub async fn start_record(device_id: String, app: AppHandle) -> Result<String, String> {
    let state = app.state::<RecordState>();
    let _operation = state.operation.lock().await;
    if state
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Err("上一段录屏仍在录制或等待保存，请先保存后再开始".to_string());
    }
    let task = tasks::begin(&app, TaskKind::Recording, &device_id, None)?;
    let remote_path = format!("/sdcard/adb_monster_rec_{}.mp4", task.id);
    // The PID belongs only to our unique recording, never to another screenrecord.
    let script = format!("screenrecord --time-limit 180 {remote_path} & recorder_pid=$!; echo $recorder_pid > {remote_path}.pid; wait $recorder_pid");
    let result = (|| {
        let adb_path = manager::find_adb().map_err(|error| error.message)?;
        prepare_command(&adb_path)
            .args(["-s", &device_id, "shell", &script])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| error.to_string())
    })();
    let mut child = match result {
        Ok(child) => child,
        Err(error) => {
            task.finish_result::<()>(&Err(error.clone()), "未启动录屏");
            return Err(error);
        }
    };
    task.artifact(&remote_path);
    *state.session.lock().map_err(|error| error.to_string())? = Some(RecordSession {
        device_id: device_id.clone(),
        remote_path: remote_path.clone(),
        task: task.clone(),
        running: true,
        finalized: false,
    });
    task.running("正在录屏，最长 180 秒；停止后保留设备文件等待保存");
    let path = remote_path.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _guard = task.guard();
        let mut finalized = false;
        let result = tokio::select! {
            biased;
            reason = task.cancelled() => {
                let signalled = interrupt_recording(&device_id, &path).await;
                if signalled.is_ok() {
                    finalized = matches!(timeout(Duration::from_secs(4), child.wait()).await, Ok(Ok(_)));
                }
                if !finalized { let _ = timeout(Duration::from_secs(2), child.kill()).await; }
                Err(reason.unwrap_or_else(|| "录屏已停止".to_string()))
            },
            result = timeout(Duration::from_secs(190), child.wait()) => {
                match result {
                    Ok(Ok(status)) if status.success() => { finalized = true; Ok(()) },
                    Ok(Ok(status)) => Err(format!("录屏异常退出：{status}")),
                    Ok(Err(error)) => Err(error.to_string()),
                    Err(_) => {
                        let _ = interrupt_recording(&device_id, &path).await;
                        let _ = timeout(Duration::from_secs(2), child.kill()).await;
                        Err("录屏进程超时".to_string())
                    },
                }
            }
        };
        if let Ok(mut session) = app.state::<RecordState>().session.lock() {
            if let Some(session) = session
                .as_mut()
                .filter(|session| session.task.id == task.id)
            {
                session.running = false;
                session.finalized = finalized;
            }
        }
        let cleanup = if finalized {
            "录制已结束，设备文件已保留；请到多媒体面板保存"
        } else {
            "设备端录屏结束状态未确认；文件已保留，请重连检查，录屏有 180 秒上限"
        };
        match result {
            Ok(()) => task.finish(TaskStatus::Completed, "录屏结束，等待保存", cleanup),
            Err(error) => task.finish_result::<()>(&Err(error), cleanup),
        }
    });
    Ok(remote_path)
}

async fn interrupt_recording(device_id: &str, path: &str) -> Result<(), String> {
    // Verify both numeric PID and our unique output path before signalling.
    let script = format!(
        "record_pid=$(cat {path}.pid 2>/dev/null); case \"$record_pid\" in ''|*[!0-9]*) exit 1;; esac; \
         if [ -r /proc/$record_pid/cmdline ]; then \
         tr '\\000' ' ' < /proc/$record_pid/cmdline | grep -F '{path}' >/dev/null || exit 1; \
         kill -2 \"$record_pid\"; fi"
    );
    timeout(Duration::from_secs(3), run_shell_raw(device_id, &script))
        .await
        .map_err(|_| "录屏停止命令超时".to_string())?
        .map(|_| ())
        .map_err(|error| error.message)
}

#[tauri::command]
pub async fn stop_record(local_path: String, app: AppHandle) -> Result<String, String> {
    if local_path.trim().is_empty() {
        return Err("Recording output path must not be empty".to_string());
    }
    let state = app.state::<RecordState>();
    let _operation = state.operation.lock().await;
    let session = state
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
        .ok_or_else(|| "没有可保存的录屏".to_string())?;
    if session.running {
        app.state::<TaskManager>()
            .cancel(&app, &session.task.id, "停止录屏并保存")?;
        tasks::wait_done(&app, &session.task.id, 12).await?;
    }
    let save_task = tasks::begin(&app, TaskKind::RecordingSave, &session.device_id, None)?;
    let _guard = save_task.guard();
    save_task.running("正在保存录屏；取消或失败时保留设备文件");
    let temporary = format!("{local_path}.{}.partial", save_task.id);
    let mut result = save_task
        .run(120, async {
            manager::pull_file(&session.device_id, &session.remote_path, &temporary)
                .await
                .map_err(|error| error.message)?;
            Ok(local_path.clone())
        })
        .await;
    // Commit is outside the cancellable scope: once published, report success.
    if result.is_ok() {
        result = if let Some(reason) = save_task.cancel_reason() {
            Err(reason)
        } else {
            tokio::fs::rename(&temporary, &local_path)
                .await
                .map(|_| local_path.clone())
                .map_err(|error| error.to_string())
        };
    }
    let temporary_cleanup = match tokio::fs::remove_file(&temporary).await {
        Ok(()) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => Some(format!("本地临时文件清理失败 {temporary}：{error}")),
    };
    let mut cleanup = "设备文件保留，可重新保存".to_string();
    if result.is_ok() {
        save_task.artifact(&local_path);
        session.task.artifact(&local_path);
        let finalized = state
            .session
            .lock()
            .map_err(|error| error.to_string())?
            .as_ref()
            .is_some_and(|session| session.finalized);
        if finalized {
            let script = format!("rm -f {} {}.pid", session.remote_path, session.remote_path);
            cleanup = match timeout(
                Duration::from_secs(5),
                run_shell_raw(&session.device_id, &script),
            )
            .await
            {
                Ok(Ok(_)) => "本地文件已保存，设备端临时录屏已清理".to_string(),
                _ => "本地文件已保存，设备临时文件清理失败，可稍后手动清理".to_string(),
            };
        }
        state
            .session
            .lock()
            .map_err(|error| error.to_string())?
            .take();
    }
    if let Some(error) = temporary_cleanup {
        cleanup.push_str(&format!("；{error}"));
    }
    save_task.finish_result(&result, &cleanup);
    result
}

#[tauri::command]
pub async fn is_recording(app: AppHandle) -> Result<bool, String> {
    Ok(get_recording_status(app).await?.running)
}

#[tauri::command]
pub async fn get_recording_status(app: AppHandle) -> Result<RecordingStatus, String> {
    let state = app.state::<RecordState>();
    let guard = state.session.lock().map_err(|error| error.to_string())?;
    Ok(RecordingStatus {
        task_id: guard.as_ref().map(|session| session.task.id.clone()),
        running: guard.as_ref().is_some_and(|session| session.running),
        has_recording: guard.is_some(),
        device_id: guard.as_ref().map(|session| session.device_id.clone()),
    })
}

#[tauri::command]
pub async fn release_recording(task_id: String, app: AppHandle) -> Result<String, String> {
    let state = app.state::<RecordState>();
    let _operation = state.operation.lock().await;
    let mut guard = state.session.lock().map_err(|error| error.to_string())?;
    let session = guard
        .as_ref()
        .ok_or_else(|| "没有等待保存的录屏".to_string())?;
    if session.task.id != task_id {
        return Err("录屏任务已变化，请刷新后重试".to_string());
    }
    if session.running {
        return Err("请先在任务中心停止录屏".to_string());
    }
    let path = session.remote_path.clone();
    guard.take();
    Ok(format!(
        "录屏占用已释放；没有删除设备文件，可从任务历史找到路径：{path}"
    ))
}

#[tauri::command]
pub async fn take_screenshot(device_id: String, output_path: String) -> Result<String, String> {
    if output_path.trim().is_empty() {
        return Err("Screenshot output path must not be empty".to_string());
    }
    manager::save_screenshot_to_file(&device_id, &output_path)
        .await
        .map_err(|e| e.message)
}

#[tauri::command]
pub async fn pull_file(device_id: String, remote: String, local: String) -> Result<String, String> {
    super::files::validate_remote_path(&remote, false)?;
    if local.trim().is_empty() {
        return Err("Local destination must not be empty".to_string());
    }
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
    if local_dir.trim().is_empty() {
        return Err("CLog output directory must not be empty".to_string());
    }
    manager::pull_clog(&device_id, &package_name, &local_dir)
        .await
        .map_err(|e| e.message)
}
