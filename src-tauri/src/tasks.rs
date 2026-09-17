//! Device-bound task lifecycle. Only metadata is persisted, never log contents or credentials.
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;
use tokio::time::{sleep, timeout, Duration};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskKind {
    Logcat,
    Recording,
    RecordingSave,
    Scrcpy,
    Diagnostic,
    WeakNetwork,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Starting,
    Running,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}

impl TaskStatus {
    pub fn active(self) -> bool {
        matches!(self, Self::Starting | Self::Running | Self::Cancelling)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub kind: TaskKind,
    pub device_id: String,
    pub target_package: Option<String>,
    pub status: TaskStatus,
    pub progress: Option<u8>,
    pub message: String,
    pub cleanup: String,
    pub output_path: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
}

struct Entry {
    record: TaskRecord,
    cancel: Option<watch::Sender<Option<String>>>,
}

impl TaskRecord {
    fn advance(&mut self, percent: Option<u8>, message: &str) {
        if matches!(self.status, TaskStatus::Starting | TaskStatus::Running) {
            self.status = TaskStatus::Running;
            self.progress = percent.map(|value| value.min(100));
            self.message = message.chars().take(2048).collect();
        }
    }

    fn complete(&mut self, status: TaskStatus, message: &str, cleanup: &str) {
        if self.status.active() {
            self.status = status;
            self.message = message.chars().take(2048).collect();
            self.cleanup = cleanup.chars().take(2048).collect();
            self.ended_at = Some(Utc::now().to_rfc3339());
            if status == TaskStatus::Completed {
                self.progress = Some(100);
            }
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub revision: u64,
    pub tasks: Vec<TaskRecord>,
    pub shutting_down: bool,
    pub storage_error: Option<String>,
}

#[derive(Default)]
struct Registry {
    entries: Vec<Entry>,
    revision: u64,
    path: Option<PathBuf>,
    storage_error: Option<String>,
}

impl Registry {
    fn cancel(&mut self, id: &str, reason: &str) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.record.id == id) else {
            return false;
        };
        if entry.record.status.active() && entry.record.status != TaskStatus::Cancelling {
            entry.record.status = TaskStatus::Cancelling;
            entry.record.message = reason.to_string();
            if let Some(sender) = &entry.cancel {
                sender.send_replace(Some(reason.to_string()));
            }
        }
        true
    }

    fn trim(&mut self) {
        let mut terminal = 0;
        self.entries.retain(|entry| {
            if entry.record.status.active() {
                true
            } else {
                terminal += 1;
                terminal <= 200
            }
        });
    }

    fn persist(&mut self) {
        self.trim();
        let Some(path) = &self.path else {
            return;
        };
        let result = (|| -> Result<(), String> {
            let records: Vec<_> = self.entries.iter().map(|entry| &entry.record).collect();
            let bytes = serde_json::to_vec(&records).map_err(|error| error.to_string())?;
            let temporary = path.with_extension("json.tmp");
            std::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
            std::fs::rename(temporary, path).map_err(|error| error.to_string())
        })();
        self.storage_error = result
            .err()
            .map(|error| format!("任务历史保存失败：{error}"));
    }

    fn snapshot(&self, shutting_down: bool) -> TaskSnapshot {
        TaskSnapshot {
            revision: self.revision,
            tasks: self
                .entries
                .iter()
                .map(|entry| entry.record.clone())
                .collect(),
            shutting_down,
            storage_error: self.storage_error.clone(),
        }
    }
}

#[derive(Default)]
pub struct TaskManager {
    registry: Mutex<Registry>,
    pub shutting_down: AtomicBool,
    pub exit_ready: AtomicBool,
}

#[derive(Clone)]
pub struct Task {
    pub id: String,
    app: AppHandle,
    cancel: watch::Receiver<Option<String>>,
}

pub struct TaskGuard(Task);

impl Drop for TaskGuard {
    fn drop(&mut self) {
        if self
            .0
            .app
            .state::<TaskManager>()
            .snapshot()
            .tasks
            .iter()
            .any(|record| record.id == self.0.id && record.status.active())
        {
            self.0.finish(
                TaskStatus::Interrupted,
                "任务执行异常中断",
                "未能确认完整清理；请检查设备和保留的文件",
            );
        }
    }
}

impl TaskManager {
    fn change(&self, app: &AppHandle, change: impl FnOnce(&mut Registry)) {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        change(&mut registry);
        registry.revision += 1;
        registry.persist();
        // Publish while serialized so older mutations cannot overwrite newer snapshots.
        let _ = app.emit(
            "tasks:changed",
            registry.snapshot(self.shutting_down.load(Ordering::Acquire)),
        );
    }

    pub fn snapshot(&self) -> TaskSnapshot {
        self.registry
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .snapshot(self.shutting_down.load(Ordering::Acquire))
    }

    pub fn current(&self, kind: TaskKind) -> Option<TaskRecord> {
        self.snapshot()
            .tasks
            .into_iter()
            .find(|record| record.kind == kind && record.status.active())
    }

    pub fn begin(
        &self,
        app: &AppHandle,
        kind: TaskKind,
        device_id: &str,
        target_package: Option<String>,
    ) -> Result<Task, String> {
        let mut result = Err("任务无法启动".to_string());
        self.change(app, |registry| {
            if self.shutting_down.load(Ordering::Acquire) {
                result = Err("正在退出，不能启动新任务".to_string());
                return;
            }
            if registry
                .entries
                .iter()
                .any(|entry| entry.record.kind == kind && entry.record.status.active())
            {
                result = Err("同类任务仍在运行或清理中，请先停止并等待完成".to_string());
                return;
            }
            let id = format!("{}-{}", Utc::now().timestamp_micros(), registry.revision);
            let (sender, receiver) = watch::channel(None);
            registry.entries.insert(
                0,
                Entry {
                    record: TaskRecord {
                        id: id.clone(),
                        kind,
                        device_id: device_id.to_string(),
                        target_package,
                        status: TaskStatus::Starting,
                        progress: None,
                        message: "正在启动".to_string(),
                        cleanup: "尚未结束".to_string(),
                        output_path: None,
                        started_at: Utc::now().to_rfc3339(),
                        ended_at: None,
                    },
                    cancel: Some(sender),
                },
            );
            result = Ok(Task {
                id,
                app: app.clone(),
                cancel: receiver,
            });
        });
        result
    }

    pub fn cancel(&self, app: &AppHandle, id: &str, reason: &str) -> Result<(), String> {
        let mut found = false;
        self.change(app, |registry| {
            found = registry.cancel(id, reason);
        });
        if found {
            Ok(())
        } else {
            Err("任务不存在或已从历史中清除".to_string())
        }
    }

    pub fn update(&self, app: &AppHandle, id: &str, update: impl FnOnce(&mut TaskRecord)) {
        self.change(app, |registry| {
            if let Some(entry) = registry
                .entries
                .iter_mut()
                .find(|entry| entry.record.id == id)
            {
                update(&mut entry.record);
                if !entry.record.status.active() {
                    entry.cancel = None;
                }
            }
        });
    }
}

impl Task {
    pub fn guard(&self) -> TaskGuard {
        TaskGuard(self.clone())
    }
    pub fn running(&self, message: &str) {
        self.progress(None, message);
    }

    pub fn progress(&self, percent: Option<u8>, message: &str) {
        self.app
            .state::<TaskManager>()
            .update(&self.app, &self.id, |record| {
                record.advance(percent, message);
            });
    }

    pub fn artifact(&self, path: &str) {
        self.app
            .state::<TaskManager>()
            .update(&self.app, &self.id, |record| {
                record.output_path = Some(path.to_string());
                if record.kind == TaskKind::Recording && !record.status.active() {
                    record.message = "录屏已保存，请查看对应保存任务的清理结果".to_string();
                }
            });
    }

    pub fn finish(&self, status: TaskStatus, message: &str, cleanup: &str) {
        self.app
            .state::<TaskManager>()
            .update(&self.app, &self.id, |record| {
                record.complete(status, message, cleanup);
            });
    }

    pub fn cancel_reason(&self) -> Option<String> {
        self.cancel.borrow().clone()
    }

    pub async fn cancelled(&self) -> Option<String> {
        wait_cancel(self.cancel.clone()).await
    }

    pub async fn run<T>(
        &self,
        seconds: u64,
        operation: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        run_cancellable(self.cancel.clone(), seconds, operation).await
    }

    pub fn finish_result<T>(&self, result: &Result<T, String>, cleanup: &str) {
        match result {
            Ok(_) => self.finish(TaskStatus::Completed, "任务完成", cleanup),
            Err(error) => self.finish(
                if self.cancel_reason().is_some() {
                    TaskStatus::Cancelled
                } else {
                    TaskStatus::Failed
                },
                error,
                cleanup,
            ),
        }
    }
}

async fn wait_cancel(mut receiver: watch::Receiver<Option<String>>) -> Option<String> {
    loop {
        if let Some(reason) = receiver.borrow_and_update().clone() {
            return Some(reason);
        }
        if receiver.changed().await.is_err() {
            return None;
        }
    }
}

async fn run_cancellable<T>(
    receiver: watch::Receiver<Option<String>>,
    seconds: u64,
    operation: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::select! {
        biased;
        reason = wait_cancel(receiver) => Err(reason.unwrap_or_else(|| "任务已结束".to_string())),
        result = timeout(Duration::from_secs(seconds), operation) => result.map_err(|_| format!("任务超时（{seconds} 秒）"))?,
    }
}

pub fn begin(
    app: &AppHandle,
    kind: TaskKind,
    device: &str,
    package: Option<String>,
) -> Result<Task, String> {
    app.state::<TaskManager>().begin(app, kind, device, package)
}

pub async fn wait_done(app: &AppHandle, id: &str, seconds: u64) -> Result<(), String> {
    timeout(Duration::from_secs(seconds), async {
        loop {
            if !app
                .state::<TaskManager>()
                .snapshot()
                .tasks
                .iter()
                .any(|record| record.id == id && record.status.active())
            {
                return;
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "任务仍在清理中，请在任务中心查看结果".to_string())
}

pub async fn stop_kind(app: &AppHandle, kind: TaskKind) -> Result<(), String> {
    if let Some(record) = app.state::<TaskManager>().current(kind) {
        app.state::<TaskManager>()
            .cancel(app, &record.id, "用户请求停止")?;
        wait_done(app, &record.id, 12).await?;
    }
    Ok(())
}

/// Owns and reaps the child; stream readers cannot outlive this supervisor.
pub fn supervise_process(
    task: Task,
    mut child: tokio::process::Child,
    device_id: String,
    logcat: bool,
) {
    use tokio::io::AsyncReadExt;
    let app = task.app.clone();
    tauri::async_runtime::spawn(async move {
        let _guard = task.guard();
        let log_app = app.clone();
        let task_id = task.id.clone();
        let stdout = child.stdout.take();
        let mut stdout_reader = tokio::spawn(async move {
            if let Some(mut stdout) = stdout {
                let mut buffer = [0_u8; 8192];
                let mut pending = Vec::new();
                while let Ok(length) = stdout.read(&mut buffer).await {
                    if length == 0 {
                        if !pending.is_empty() {
                            let _ = log_app.emit("logcat:batch", serde_json::json!({ "taskId": task_id, "deviceId": device_id, "lines": [String::from_utf8_lossy(&pending)] }));
                        }
                        break;
                    }
                    pending.extend_from_slice(&buffer[..length]);
                    let mut batch = Vec::new();
                    while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                        batch.push(
                            String::from_utf8_lossy(&pending[..end])
                                .trim_end_matches('\r')
                                .to_string(),
                        );
                        pending.drain(..=end);
                    }
                    if pending.len() > 32 * 1024 {
                        batch.push(String::from_utf8_lossy(&pending).into_owned());
                        pending.clear();
                    }
                    if !batch.is_empty() {
                        let _ = log_app.emit("logcat:batch", serde_json::json!({ "taskId": task_id, "deviceId": device_id, "lines": batch }));
                    }
                }
            }
        });
        let stderr = child.stderr.take();
        let mut stderr_reader = tokio::spawn(async move {
            let mut tail = Vec::new();
            if let Some(mut stderr) = stderr {
                let mut buffer = [0_u8; 1024];
                while let Ok(length) = stderr.read(&mut buffer).await {
                    if length == 0 {
                        break;
                    }
                    tail.extend_from_slice(&buffer[..length]);
                    if tail.len() > 8192 {
                        tail.drain(..tail.len() - 8192);
                    }
                }
            }
            String::from_utf8_lossy(&tail).into_owned()
        });
        task.running("正在运行");
        let (result, cleanup) = tokio::select! {
            biased;
            reason = task.cancelled() => {
                let killed = timeout(Duration::from_secs(3), child.kill()).await;
                let cleanup = match killed { Ok(Ok(())) => "子进程已停止并回收".to_string(), other => format!("子进程停止未确认：{other:?}") };
                (Err(reason.unwrap_or_else(|| "任务已结束".to_string())), cleanup)
            },
            result = child.wait() => {
                let result = result.map_err(|error| error.to_string()).and_then(|status| if status.success() { Ok(()) } else { Err(format!("子进程异常退出：{status}")) });
                (result, "子进程已退出".to_string())
            }
        };
        if timeout(Duration::from_secs(1), &mut stdout_reader)
            .await
            .is_err()
        {
            stdout_reader.abort();
            let _ = stdout_reader.await;
        }
        let detail = match timeout(Duration::from_secs(1), &mut stderr_reader).await {
            Ok(Ok(detail)) => detail,
            _ => {
                stderr_reader.abort();
                let _ = stderr_reader.await;
                String::new()
            }
        };
        let result = result.map_err(|error| {
            if task.cancel_reason().is_none() && !detail.trim().is_empty() {
                format!("{error}\n{}", detail.trim())
            } else {
                error
            }
        });
        task.finish_result(&result, &cleanup);
        let stopped_event = if logcat {
            "logcat:stopped"
        } else {
            "scrcpy:stopped"
        };
        if let Err(error) = &result {
            if task.cancel_reason().is_none() {
                let _ = app.emit(
                    if logcat {
                        "logcat:error"
                    } else {
                        "scrcpy:error"
                    },
                    error,
                );
            }
        }
        let _ = app.emit(stopped_event, task.id);
    });
}

pub fn initialize(app: &AppHandle) {
    let manager = app.state::<TaskManager>();
    manager.change(app, |registry| {
        let result = (|| -> Result<(), String> {
            let directory = app
                .path()
                .app_local_data_dir()
                .map_err(|error| error.to_string())?;
            std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
            let path = directory.join("task-history.json");
            if path.exists() {
                let size = std::fs::metadata(&path)
                    .map_err(|error| error.to_string())?
                    .len();
                if size > 4 * 1024 * 1024 {
                    return Err("任务历史文件过大，未加载".to_string());
                }
                let mut records: Vec<TaskRecord> = serde_json::from_slice(
                    &std::fs::read(&path).map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                recover_records(&mut records);
                registry.entries = records
                    .into_iter()
                    .take(200)
                    .map(|record| Entry {
                        record,
                        cancel: None,
                    })
                    .collect();
            }
            registry.path = Some(path);
            Ok(())
        })();
        registry.storage_error = result.err();
    });
    let monitor_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut misses: HashMap<String, u8> = HashMap::new();
        loop {
            sleep(Duration::from_secs(3)).await;
            let manager = monitor_app.state::<TaskManager>();
            if manager.shutting_down.load(Ordering::Acquire) {
                return;
            }
            let active: Vec<_> = manager
                .snapshot()
                .tasks
                .into_iter()
                .filter(|record| record.status.active())
                .collect();
            if active.is_empty() {
                misses.clear();
                continue;
            }
            let output = crate::adb::process::run_adb_command_with_timeout(
                &["devices"],
                Duration::from_secs(5),
            )
            .await;
            let online: HashSet<_> = output
                .map(|text| {
                    crate::adb::parser::parse_devices_output(&text)
                        .into_iter()
                        .filter(|device| device.status == "device")
                        .map(|device| device.id)
                        .collect()
                })
                .unwrap_or_default();
            let active_ids: HashSet<_> = active.iter().map(|record| record.id.clone()).collect();
            misses.retain(|id, _| active_ids.contains(id));
            for record in active {
                if online.contains(&record.device_id) {
                    misses.remove(&record.id);
                    continue;
                }
                let missed = misses.entry(record.id.clone()).or_default();
                *missed = missed.saturating_add(1);
                if *missed >= 2 && record.status != TaskStatus::Cancelling {
                    let _ = manager.cancel(
                        &monitor_app,
                        &record.id,
                        "设备断开、未授权或 ADB 不可用，正在停止任务",
                    );
                }
            }
        }
    });
}

fn recover_records(records: &mut [TaskRecord]) {
    for record in records.iter_mut().filter(|record| record.status.active()) {
        record.status = TaskStatus::Interrupted;
        record.ended_at = Some(Utc::now().to_rfc3339());
        record.message = "上次程序异常退出，任务未正常结束".to_string();
        record.cleanup =
            "设备端状态未确认；录屏文件可能仍在设备上，VPN 请检查助手或等待到期恢复".to_string();
    }
}

#[tauri::command]
pub fn list_tasks(app: AppHandle) -> TaskSnapshot {
    app.state::<TaskManager>().snapshot()
}

#[tauri::command]
pub fn cancel_task(task_id: String, app: AppHandle) -> Result<(), String> {
    app.state::<TaskManager>()
        .cancel(&app, &task_id, "用户请求停止")
}

#[tauri::command]
pub fn clear_task_history(app: AppHandle) {
    app.state::<TaskManager>().change(&app, |registry| {
        registry
            .entries
            .retain(|entry| entry.record.status.active())
    });
}

pub fn shutdown(app: &AppHandle) {
    let manager = app.state::<TaskManager>();
    if manager.shutting_down.swap(true, Ordering::AcqRel) {
        return;
    }
    for record in manager
        .snapshot()
        .tasks
        .iter()
        .filter(|record| record.status.active())
    {
        let _ = manager.cancel(app, &record.id, "程序退出，正在清理任务");
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = timeout(Duration::from_secs(25), async {
            while app
                .state::<TaskManager>()
                .snapshot()
                .tasks
                .iter()
                .any(|record| record.status.active())
            {
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await;
        let manager = app.state::<TaskManager>();
        manager.change(&app, |registry| {
            for entry in registry
                .entries
                .iter_mut()
                .filter(|entry| entry.record.status.active())
            {
                entry.record.status = TaskStatus::Interrupted;
                entry.record.message = "退出清理超时".to_string();
                entry.record.cleanup =
                    "未确认设备端状态，请重连检查；原始录屏文件不会删除".to_string();
                entry.record.ended_at = Some(Utc::now().to_rfc3339());
            }
        });
        manager.exit_ready.store(true, Ordering::Release);
        app.exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(status: TaskStatus) -> TaskRecord {
        TaskRecord {
            id: "test".into(),
            kind: TaskKind::Logcat,
            device_id: "a".into(),
            target_package: None,
            status,
            progress: None,
            message: String::new(),
            cleanup: String::new(),
            output_path: None,
            started_at: String::new(),
            ended_at: None,
        }
    }

    #[test]
    fn restart_marks_active_tasks_interrupted_without_changing_completed_tasks() {
        let mut records = vec![
            record(TaskStatus::Running),
            record(TaskStatus::Cancelling),
            record(TaskStatus::Completed),
        ];
        recover_records(&mut records);
        assert_eq!(records[0].status, TaskStatus::Interrupted);
        assert_eq!(records[1].status, TaskStatus::Interrupted);
        assert_eq!(records[2].status, TaskStatus::Completed);
    }

    #[test]
    fn bounded_history_never_evicts_active_tasks() {
        let mut registry = Registry::default();
        for _ in 0..220 {
            registry.entries.push(Entry {
                record: record(TaskStatus::Completed),
                cancel: None,
            });
        }
        registry.entries.push(Entry {
            record: record(TaskStatus::Running),
            cancel: None,
        });
        registry.trim();
        assert_eq!(registry.entries.len(), 201);
        assert!(registry.entries.last().unwrap().record.status.active());
    }

    #[test]
    fn old_task_cancellation_never_reaches_a_new_task_on_another_device() {
        let (sender, receiver) = watch::channel(None);
        let mut old = record(TaskStatus::Completed);
        old.id = "old".into();
        let mut new = record(TaskStatus::Running);
        new.id = "new".into();
        new.device_id = "b".into();
        let mut registry = Registry {
            entries: vec![
                Entry {
                    record: old,
                    cancel: None,
                },
                Entry {
                    record: new,
                    cancel: Some(sender),
                },
            ],
            ..Registry::default()
        };
        assert!(registry.cancel("old", "stop old"));
        assert!(!registry.cancel("missing", "stop missing"));
        assert!(receiver.borrow().is_none());
        assert_eq!(registry.entries[1].record.status, TaskStatus::Running);
        assert!(registry.cancel("new", "stop new"));
        assert!(registry.cancel("new", "second request"));
        assert_eq!(receiver.borrow().as_deref(), Some("stop new"));
    }

    #[test]
    fn late_progress_and_completion_cannot_revive_a_terminal_task() {
        let mut value = record(TaskStatus::Cancelling);
        value.advance(Some(70), "late progress");
        assert_eq!(value.status, TaskStatus::Cancelling);
        value.complete(TaskStatus::Cancelled, "cancelled", "cleaned");
        let ended = value.ended_at.clone();
        value.advance(Some(90), "later progress");
        value.complete(TaskStatus::Completed, "late success", "late cleanup");
        assert_eq!(value.status, TaskStatus::Cancelled);
        assert_eq!(value.message, "cancelled");
        assert_eq!(value.cleanup, "cleaned");
        assert_eq!(value.ended_at, ended);
    }

    #[tokio::test]
    async fn already_cancelled_task_never_starts_its_operation() {
        let (_sender, receiver) = watch::channel(Some("cancelled".to_string()));
        let called = AtomicBool::new(false);
        let result = run_cancellable(receiver, 5, async {
            called.store(true, Ordering::Release);
            Ok(())
        })
        .await;
        assert_eq!(result.unwrap_err(), "cancelled");
        assert!(!called.load(Ordering::Acquire));
    }

    #[tokio::test(start_paused = true)]
    async fn total_timeout_drops_the_blocked_operation() {
        struct Resource(std::sync::Arc<AtomicBool>);
        impl Drop for Resource {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let (_sender, receiver) = watch::channel(None);
        let dropped = std::sync::Arc::new(AtomicBool::new(false));
        let resource = Resource(dropped.clone());
        let started = tokio::time::Instant::now();
        let result = run_cancellable(receiver, 5, async move {
            let _resource = resource;
            std::future::pending::<Result<(), String>>().await
        })
        .await;
        assert!(result.unwrap_err().contains("超时"));
        assert_eq!(started.elapsed(), Duration::from_secs(5));
        assert!(dropped.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_blocked_write() {
        use tokio::io::AsyncWriteExt;
        let (sender, receiver) = watch::channel(None);
        let (mut writer, _reader) = tokio::io::duplex(1);
        let worker = tokio::spawn(async move {
            run_cancellable(receiver, 60, async {
                writer
                    .write_all(&[1; 128])
                    .await
                    .map_err(|error| error.to_string())
            })
            .await
        });
        tokio::task::yield_now().await;
        sender.send_replace(Some("stop".to_string()));
        assert_eq!(
            timeout(Duration::from_secs(1), worker)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err(),
            "stop"
        );
    }

    #[tokio::test]
    async fn ending_a_task_releases_its_cancellation_watcher() {
        let (sender, receiver) = watch::channel(None);
        drop(sender);
        assert!(wait_cancel(receiver).await.is_none());
    }

    #[test]
    fn history_can_atomically_replace_an_existing_file() {
        let directory = std::env::temp_dir().join(format!(
            "adb-monster-history-test-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("task-history.json");
        let mut registry = Registry {
            path: Some(path.clone()),
            ..Registry::default()
        };
        registry.entries.push(Entry {
            record: record(TaskStatus::Running),
            cancel: None,
        });
        registry.persist();
        assert!(registry.storage_error.is_none());
        registry.entries[0]
            .record
            .complete(TaskStatus::Completed, "done", "cleaned");
        registry.persist();
        assert!(registry.storage_error.is_none());
        let records: Vec<TaskRecord> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(records[0].status, TaskStatus::Completed);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
