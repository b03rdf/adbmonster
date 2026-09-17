use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use tauri::{AppHandle, Emitter, Manager};

use crate::adb::{manager, process};
use crate::network_proxy::{self, ProxyHandle};
use crate::network_scenario::{NetworkReport, NetworkRuntime, NetworkScenario};
use crate::tasks::{self, Task, TaskKind, TaskStatus};
use crate::types::{WeakNetworkCapabilities, WeakNetworkConfig, WeakNetworkStatus};

const HELPER_PACKAGE: &str = "com.rendongfang.adbmonster.vpnhelper";
const HELPER_VERSION: &str = "1.0.3";
const CONTROL_COMPONENT: &str = "com.rendongfang.adbmonster.vpnhelper/.ControlActivity";
const STATUS_COMPONENT: &str = "com.rendongfang.adbmonster.vpnhelper/.StatusReceiver";
const ACTION_AUTHORIZE: &str = "com.rendongfang.adbmonster.vpnhelper.AUTHORIZE";
const ACTION_APPLY: &str = "com.rendongfang.adbmonster.vpnhelper.APPLY";
const ACTION_STOP: &str = "com.rendongfang.adbmonster.vpnhelper.STOP";
const ACTION_STATUS: &str = "com.rendongfang.adbmonster.vpnhelper.STATUS";
const DEVICE_PROXY_PORT: u16 = 38_991;
const HELPER_APK: &[u8] = include_bytes!("../../resources/adb-monster-vpn-helper.apk");

struct WeakNetworkSession {
    task: Task,
    device_id: String,
    target_package: String,
    expires_at: String,
    proxy: ProxyHandle,
}

#[derive(Clone)]
struct SessionMetadata {
    device_id: String,
    target_package: String,
    expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HelperStatus {
    running: bool,
    authorized: bool,
    version: String,
    target_package: Option<String>,
    expires_at_millis: Option<i64>,
}

#[derive(Default)]
pub struct WeakNetworkState {
    pub(super) last_report: Mutex<Option<NetworkReport>>,
    next_id: AtomicU64,
    session: Mutex<Option<WeakNetworkSession>>,
    operation: tokio::sync::Mutex<()>,
}

#[tauri::command]
pub async fn detect_weak_network_capabilities(
    device_id: String,
) -> Result<WeakNetworkCapabilities, String> {
    detect_capabilities(&device_id).await
}

#[tauri::command]
pub async fn install_weak_network_helper(
    device_id: String,
) -> Result<WeakNetworkCapabilities, String> {
    let temp_path = std::env::temp_dir().join(format!(
        "adb-monster-vpn-helper-{}-{}.apk",
        std::process::id(),
        Utc::now().timestamp_millis()
    ));
    std::fs::write(&temp_path, HELPER_APK)
        .map_err(|error| format!("写入临时辅助 APK 失败：{error}"))?;
    let path = temp_path.to_string_lossy().to_string();
    let install_result = manager::install_apk(&device_id, &path)
        .await
        .map_err(|error| error.message);
    let _ = std::fs::remove_file(&temp_path);
    install_result?;
    detect_capabilities(&device_id).await
}

#[tauri::command]
pub async fn authorize_weak_network_helper(device_id: String) -> Result<String, String> {
    if !helper_installed(&device_id).await {
        return Err("请先安装弱网辅助 APK".to_string());
    }
    launch_control_activity(&device_id, ACTION_AUTHORIZE, &[]).await?;
    Ok("请在设备上确认 VPN 授权，然后返回工具继续操作".to_string())
}

#[tauri::command]
pub async fn apply_weak_network(
    device_id: String,
    config: WeakNetworkConfig,
    app: AppHandle,
) -> Result<WeakNetworkStatus, String> {
    validate_config(&config)?;
    run_weak_network_scenario(device_id, NetworkScenario::single(&config), app).await
}

#[tauri::command]
pub async fn run_weak_network_scenario(
    device_id: String,
    scenario: NetworkScenario,
    app: AppHandle,
) -> Result<WeakNetworkStatus, String> {
    scenario.validate()?;
    let config = scenario.initial_config();
    let state = app.state::<WeakNetworkState>();
    let _operation = state.operation.lock().await;
    if let Some(previous) = take_session(&app)? {
        restore_session(&app, previous, "被新的弱网场景替换")
            .await
            .map_err(|error| format!("停止上一轮弱网失败，新配置尚未启动：{error}"))?;
    }
    let task = tasks::begin(
        &app,
        TaskKind::WeakNetwork,
        &device_id,
        Some(config.target_package.clone()),
    )?;
    let result = task
        .run(
            90,
            apply_inner(
                device_id.clone(),
                config,
                scenario,
                app.clone(),
                task.clone(),
            ),
        )
        .await;
    if let Err(error) = &result {
        let cleanup = cleanup_device(&device_id).await;
        task.finish_result::<()>(
            &Err(error.clone()),
            &match cleanup {
                Ok(()) => "本地代理已停止，VPN 和反向映射已清理".to_string(),
                Err(error) => {
                    format!("本地代理已停止，设备端清理未确认：{error}；请检查助手或等待到期恢复")
                }
            },
        );
        let _ = app.emit("weak-network:status", inactive_status(error));
    }
    result
}

async fn apply_inner(
    device_id: String,
    config: WeakNetworkConfig,
    scenario: NetworkScenario,
    app: AppHandle,
    task: Task,
) -> Result<WeakNetworkStatus, String> {
    task.running("正在检查助手并启动 VPN");
    if !package_installed(&device_id, &config.target_package).await {
        return Err(format!("目标应用 {} 未安装", config.target_package));
    }
    let capabilities = detect_capabilities(&device_id).await?;
    if !capabilities.supported {
        return Err(capabilities.message);
    }

    if capabilities.helper_running {
        stop_helper(&device_id)
            .await
            .map_err(|error| format!("停止设备端残留 VPN 失败：{error}"))?;
    }

    let id = app
        .state::<WeakNetworkState>()
        .next_id
        .fetch_add(1, Ordering::AcqRel)
        + 1;
    let username = "adbmonster".to_string();
    let password = format!(
        "{:016x}{:016x}",
        id,
        Utc::now().timestamp_nanos_opt().unwrap_or_default() as u64
    );
    let runtime = NetworkRuntime::new(scenario, task.id.clone(), device_id.clone());
    let proxy = network_proxy::start_proxy_observed(
        config.clone(),
        username.clone(),
        password.clone(),
        runtime.clone(),
    )
    .await?;

    let _ = remove_reverse(&device_id).await;
    if let Err(error) = create_reverse(&device_id, proxy.port).await {
        proxy.stop();
        return Err(error);
    }

    let extras = vec![
        ("--es", "target_package", config.target_package.clone()),
        ("--es", "proxy_username", username),
        ("--es", "proxy_password", password),
        ("--ei", "proxy_port", DEVICE_PROXY_PORT.to_string()),
        (
            "--ei",
            "duration_seconds",
            (config.duration_seconds + 90).to_string(),
        ),
    ];
    if let Err(error) = launch_control_activity(&device_id, ACTION_APPLY, &extras).await {
        proxy.stop();
        let _ = remove_reverse(&device_id).await;
        return Err(error);
    }

    let mut helper_status = None;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if let Ok(status) = query_helper_status(&device_id).await {
            if status.running
                && status.target_package.as_deref() == Some(config.target_package.as_str())
            {
                helper_status = Some(status);
                break;
            }
            if !status.authorized {
                break;
            }
        }
    }
    let Some(helper_status) = helper_status else {
        proxy.stop();
        let _ = launch_control_activity(&device_id, ACTION_STOP, &[]).await;
        let _ = remove_reverse(&device_id).await;
        return Err("辅助 APK 未能启动 VPN，请确认授权状态并重试".to_string());
    };

    let _ = helper_status;
    let expires_at = (Utc::now() + chrono::Duration::seconds(i64::from(config.duration_seconds)))
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    runtime.activate();
    let session = WeakNetworkSession {
        task: task.clone(),
        device_id: device_id.clone(),
        target_package: config.target_package.clone(),
        expires_at: expires_at.clone(),
        proxy,
    };
    *app.state::<WeakNetworkState>()
        .session
        .lock()
        .map_err(|error| error.to_string())? = Some(session);

    schedule_scenario(app.clone(), task, runtime);
    let status = WeakNetworkStatus {
        active: true,
        device_id: Some(device_id),
        target_package: Some(config.target_package),
        expires_at: Some(expires_at),
        message: "VPN 弱网已生效；TCP 模拟带宽/延迟，UDP 额外模拟丢包、重复和乱序".to_string(),
    };
    let _ = app.emit("weak-network:status", status.clone());
    Ok(status)
}

#[tauri::command]
pub async fn stop_weak_network(
    device_id: String,
    app: AppHandle,
) -> Result<WeakNetworkStatus, String> {
    let state = app.state::<WeakNetworkState>();
    let _operation = state.operation.lock().await;
    let active_device = session_metadata(&app)?.map(|session| session.device_id);
    if active_device.as_deref() == Some(device_id.as_str()) {
        if let Some(session) = take_session(&app)? {
            restore_session(&app, session, "用户手动停止").await?;
        }
    } else {
        cleanup_device(&device_id).await?;
    }

    let status = if let Some(session) = session_metadata(&app)? {
        status_from_metadata(&session, "其他设备的弱网测试仍在运行")
    } else {
        inactive_status("VPN 弱网已停止，网络恢复正常")
    };
    let _ = app.emit("weak-network:status", status.clone());
    Ok(status)
}

#[tauri::command]
pub async fn get_weak_network_status(
    device_id: String,
    app: AppHandle,
) -> Result<WeakNetworkStatus, String> {
    let state = app.state::<WeakNetworkState>();
    let _operation = state.operation.lock().await;
    if let Some(session) = session_metadata(&app)? {
        if session.device_id == device_id {
            let helper = query_helper_status(&device_id).await?;
            if !helper.running {
                remove_reverse(&device_id).await?;
                if let Some(stopped) = take_session(&app)? {
                    finalize_report(&app, &stopped.proxy, "设备端 VPN 已停止，场景提前结束").await;
                    stopped.task.finish(
                        TaskStatus::Completed,
                        "设备端 VPN 已停止",
                        "本地代理和反向映射已清理，设备 VPN 网卡已释放",
                    );
                }
                return Ok(inactive_status("设备端 VPN 已停止，网络恢复正常"));
            }
        }
        return Ok(status_from_metadata(&session, "VPN 弱网正在运行"));
    }

    if helper_installed(&device_id).await {
        let helper = query_helper_status(&device_id).await?;
        if helper.running {
            return Ok(WeakNetworkStatus {
                active: true,
                device_id: Some(device_id),
                target_package: helper.target_package,
                expires_at: helper.expires_at_millis.and_then(format_millis),
                message: "检测到设备端残留的 VPN 弱网，可点击停止恢复".to_string(),
            });
        }
    }
    Ok(inactive_status("当前未启用 VPN 弱网"))
}

async fn detect_capabilities(device_id: &str) -> Result<WeakNetworkCapabilities, String> {
    if !helper_installed(device_id).await {
        return Ok(WeakNetworkCapabilities {
            supported: false,
            helper_installed: false,
            helper_update_required: false,
            vpn_authorized: false,
            helper_running: false,
            helper_version: None,
            active_target_package: None,
            expires_at: None,
            message: "需要先安装内置的非 Root VPN 弱网助手".to_string(),
        });
    }

    let helper = match query_helper_status(device_id).await {
        Ok(status) => status,
        Err(error) => {
            return Ok(WeakNetworkCapabilities {
                supported: false,
                helper_installed: true,
                helper_update_required: true,
                vpn_authorized: false,
                helper_running: false,
                helper_version: None,
                active_target_package: None,
                expires_at: None,
                message: format!("辅助 APK 无法通信，请重新安装：{error}"),
            });
        }
    };
    let update_required = helper.version != HELPER_VERSION;
    let supported = helper.authorized && !update_required;
    let message = if update_required {
        format!(
            "辅助 APK 版本 {} 与桌面端不匹配，需要更新到 {}",
            helper.version, HELPER_VERSION
        )
    } else if !helper.authorized {
        "辅助 APK 已安装，需要在设备上确认一次 VPN 授权".to_string()
    } else if helper.running {
        format!(
            "VPN 弱网正在作用于 {}",
            helper.target_package.as_deref().unwrap_or("未知应用")
        )
    } else {
        "非 Root VPN 弱网已就绪；只接管所选应用流量".to_string()
    };

    Ok(WeakNetworkCapabilities {
        supported,
        helper_installed: true,
        helper_update_required: update_required,
        vpn_authorized: helper.authorized,
        helper_running: helper.running,
        helper_version: Some(helper.version),
        active_target_package: helper.target_package,
        expires_at: helper.expires_at_millis.and_then(format_millis),
        message,
    })
}

async fn helper_installed(device_id: &str) -> bool {
    package_installed(device_id, HELPER_PACKAGE).await
}

async fn package_installed(device_id: &str, package_name: &str) -> bool {
    let args = ["pm", "path", package_name];
    run_shell_args_timeout(device_id, &args, 8)
        .await
        .is_ok_and(|output| output.lines().any(|line| line.starts_with("package:")))
}

async fn query_helper_status(device_id: &str) -> Result<HelperStatus, String> {
    let args = [
        "am",
        "broadcast",
        "--receiver-foreground",
        "--include-stopped-packages",
        "-n",
        STATUS_COMPONENT,
        "-a",
        ACTION_STATUS,
    ];
    let output = run_shell_args_timeout(device_id, &args, 8).await?;
    let data =
        extract_broadcast_data(&output).ok_or_else(|| format!("辅助 APK 未返回状态：{output}"))?;
    let mut status = parse_helper_status_data(data)?;
    // Older helpers could clear their flag without closing the system-bound VPN.
    // Require the app's dedicated VPN addresses to disappear before reporting
    // stopped; never infer recovery from a flag or a failed interface query.
    if !status.running {
        status.running = helper_vpn_interface_present(device_id).await?;
    }
    Ok(status)
}

async fn helper_vpn_interface_present(device_id: &str) -> Result<bool, String> {
    let output = run_shell_args_timeout(device_id, &["ip", "-o", "addr", "show"], 3)
        .await
        .map_err(|error| format!("检查设备 VPN 网卡失败：{error}"))?;
    if !output.lines().any(|line| {
        line.split_whitespace().next().is_some_and(|index| {
            index
                .strip_suffix(':')
                .is_some_and(|number| number.parse::<u32>().is_ok())
        })
    }) {
        return Err("设备未返回可识别的网卡列表，VPN 关闭状态未确认".to_string());
    }
    Ok(has_helper_vpn_address(&output))
}

fn has_helper_vpn_address(output: &str) -> bool {
    output.split_whitespace().any(|field| {
        field == "10.111.222.1/30" || field.eq_ignore_ascii_case("fd00:111:222::1/126")
    })
}

fn extract_broadcast_data(output: &str) -> Option<&str> {
    if let Some(start) = output.rfind("data=\"") {
        let value = &output[start + 6..];
        return value.find('"').map(|end| &value[..end]);
    }
    let start = output.rfind("data=")?;
    let value = &output[start + 5..];
    let end = value
        .find(|character| ['\r', '\n', ','].contains(&character))
        .unwrap_or(value.len());
    Some(value[..end].trim())
}

fn parse_helper_status_data(data: &str) -> Result<HelperStatus, String> {
    let fields = data.split('|').collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err("辅助 APK 状态格式无效".to_string());
    }
    let running = fields[0] == "1";
    let authorized = fields[1] == "1";
    if !matches!(fields[0], "0" | "1") || !matches!(fields[1], "0" | "1") {
        return Err("辅助 APK 状态标志无效".to_string());
    }
    let expires_at_millis = fields[4].parse::<i64>().ok().filter(|value| *value > 0);
    Ok(HelperStatus {
        running,
        authorized,
        version: fields[2].to_string(),
        target_package: (!fields[3].is_empty()).then(|| fields[3].to_string()),
        expires_at_millis,
    })
}

async fn launch_control_activity(
    device_id: &str,
    action: &str,
    extras: &[(&str, &str, String)],
) -> Result<(), String> {
    let owned = control_activity_args(action, extras);
    let args = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let label = match action {
        ACTION_STOP => "停止 VPN",
        ACTION_APPLY => "启动 VPN",
        _ => "VPN 授权",
    };
    let output = run_shell_args_timeout(device_id, &args, 10)
        .await
        .map_err(|error| format!("发送{label}请求失败：{error}"))?;
    if output.contains("Error:")
        || output.contains("Exception")
        || output.contains("Permission Denial")
    {
        Err(format!("发送{label}请求失败：{output}"))
    } else {
        Ok(())
    }
}

fn control_activity_args(action: &str, extras: &[(&str, &str, String)]) -> Vec<String> {
    // ControlActivity calls finish() immediately after dispatching the action.
    // Waiting with am start -W can time out even when the VPN action succeeded.
    // Confirm VPN state separately from activity dispatch.
    let mut owned = vec![
        "am".to_string(),
        "start".to_string(),
        "-n".to_string(),
        CONTROL_COMPONENT.to_string(),
        "-a".to_string(),
        action.to_string(),
    ];
    for (kind, key, value) in extras {
        owned.push((*kind).to_string());
        owned.push((*key).to_string());
        owned.push(value.clone());
    }
    owned
}

async fn stop_helper(device_id: &str) -> Result<(), String> {
    let request = launch_control_activity(device_id, ACTION_STOP, &[]).await;
    confirm_helper_stopped(request, || query_helper_status(device_id)).await
}

async fn confirm_helper_stopped<F, Fut>(
    request: Result<(), String>,
    mut query: F,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<HelperStatus, String>>,
{
    // A command timeout does not prove that the device rejected the request.
    // Always read back the state, but keep cleanup within the shutdown budget.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let mut last_status = "VPN 停止状态未确认".to_string();
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, query()).await {
            Ok(Ok(status)) if !status.running => return Ok(()),
            Ok(Ok(_)) => last_status = "设备端 VPN 仍在运行".to_string(),
            Ok(Err(error)) => last_status = format!("读取 VPN 状态失败：{error}"),
            Err(_) => {
                last_status = format!("VPN 停止确认超时（8 秒）；{last_status}");
                break;
            }
        }
        tokio::time::sleep_until(std::cmp::min(
            deadline,
            tokio::time::Instant::now() + Duration::from_millis(200),
        ))
        .await;
    }
    Err(match request {
        Ok(()) => last_status,
        Err(error) => format!("{error}；{last_status}"),
    })
}

async fn create_reverse(device_id: &str, host_port: u16) -> Result<(), String> {
    let device = format!("tcp:{DEVICE_PROXY_PORT}");
    let host = format!("tcp:{host_port}");
    run_adb_timeout(&["-s", device_id, "reverse", &device, &host], 8)
        .await
        .map(|_| ())
        .map_err(|error| format!("创建 ADB 反向代理失败：{error}"))
}

async fn remove_reverse(device_id: &str) -> Result<(), String> {
    let device = format!("tcp:{DEVICE_PROXY_PORT}");
    match run_adb_timeout(&["-s", device_id, "reverse", "--remove", &device], 8).await {
        Ok(_) => Ok(()),
        Err(error) if error.contains(&device) && error.contains("not found") => Ok(()),
        Err(error) => Err(error),
    }
}

async fn cleanup_device(device_id: &str) -> Result<(), String> {
    let (helper, reverse) = tokio::join!(stop_helper(device_id), remove_reverse(device_id));
    device_cleanup_result(helper, reverse)
}

fn device_cleanup_result(
    helper: Result<(), String>,
    reverse: Result<(), String>,
) -> Result<(), String> {
    match (helper, reverse) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(format!("VPN 停止未确认：{error}；ADB 反向映射已清理")),
        (Ok(()), Err(error)) => Err(format!("VPN 已停止；清理 ADB 反向映射失败：{error}")),
        (Err(helper), Err(reverse)) => Err(format!(
            "VPN 停止未确认：{helper}；清理 ADB 反向映射失败：{reverse}"
        )),
    }
}

async fn restore_session(
    app: &AppHandle,
    session: WeakNetworkSession,
    reason: &str,
) -> Result<(), String> {
    let _guard = session.task.guard();
    let reason = session
        .task
        .cancel_reason()
        .unwrap_or_else(|| reason.to_string());
    finalize_report(app, &session.proxy, &reason).await;
    let result = cleanup_device(&session.device_id).await;
    match &result {
        Ok(()) => session.task.finish(
            if session.task.cancel_reason().is_some() {
                TaskStatus::Cancelled
            } else {
                TaskStatus::Completed
            },
            "弱网测试已结束",
            "本地代理已停止，设备 VPN 和反向映射已清理",
        ),
        Err(error) => session.task.finish(
            TaskStatus::Failed,
            "弱网已停止，但设备端恢复未确认",
            &format!("本地代理已停止；{error}；请检查助手或等待设备端到期恢复"),
        ),
    }
    let _ = app.emit(
        "weak-network:status",
        inactive_status(if result.is_ok() {
            "VPN 弱网已停止，网络恢复正常"
        } else {
            "本地代理已停止，设备端恢复未确认，请查看任务中心"
        }),
    );
    result
}

async fn stop_managed(app: AppHandle, task_id: &str, reason: &str) {
    let state = app.state::<WeakNetworkState>();
    let _operation = state.operation.lock().await;
    let session = {
        let Ok(mut session) = state.session.lock() else {
            return;
        };
        if session
            .as_ref()
            .is_some_and(|session| session.task.id == task_id)
        {
            session.take()
        } else {
            None
        }
    };
    if let Some(session) = session {
        let _ = restore_session(&app, session, reason).await;
    }
}

fn schedule_scenario(app: AppHandle, task: Task, runtime: std::sync::Arc<NetworkRuntime>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut phase = usize::MAX;
        loop {
            tokio::select! {
                biased;
                reason = task.cancelled() => {
                    if reason.is_some() { stop_managed(app, &task.id, "任务已取消").await; }
                    return;
                },
                _ = interval.tick() => {
                    if runtime.tick() { stop_managed(app, &task.id, "场景按计划执行完毕").await; return; }
                    let current = runtime.current();
                    if phase != current.generation {
                        phase = current.generation;
                        task.running(&format!("阶段 {}：{}{}", phase + 1, current.phase.name, if current.phase.offline { "（短时断网）" } else { "" }));
                    }
                }
            }
        }
    });
}

async fn finalize_report(app: &AppHandle, proxy: &ProxyHandle, reason: &str) {
    let warning = proxy.stop_and_wait().await.err();
    let reason = warning
        .map(|error| format!("{reason}；{error}"))
        .unwrap_or_else(|| reason.to_string());
    let report = proxy.runtime.finish(&reason);
    *app.state::<WeakNetworkState>()
        .last_report
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(report.clone());
    if let Err(error) = super::network_profiles::persist_report(app, &report).await {
        let state = app.state::<WeakNetworkState>();
        let mut guard = state
            .last_report
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(report) = guard.as_mut() {
            report
                .interpretation
                .push(format!("报告自动保存失败，可手动导出：{error}"));
        }
    }
}

fn take_session<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Option<WeakNetworkSession>, String> {
    app.state::<WeakNetworkState>()
        .session
        .lock()
        .map_err(|error| error.to_string())
        .map(|mut session| session.take())
}

pub(super) fn active_report(app: &AppHandle) -> Option<NetworkReport> {
    let state = app.state::<WeakNetworkState>();
    let guard = state
        .session
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    guard
        .as_ref()
        .map(|session| session.proxy.runtime.snapshot())
}

fn session_metadata<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Option<SessionMetadata>, String> {
    let state = app.state::<WeakNetworkState>();
    let session = state.session.lock().map_err(|error| error.to_string())?;
    Ok(session.as_ref().map(|session| SessionMetadata {
        device_id: session.device_id.clone(),
        target_package: session.target_package.clone(),
        expires_at: session.expires_at.clone(),
    }))
}

fn status_from_metadata(session: &SessionMetadata, message: &str) -> WeakNetworkStatus {
    WeakNetworkStatus {
        active: true,
        device_id: Some(session.device_id.clone()),
        target_package: Some(session.target_package.clone()),
        expires_at: Some(session.expires_at.clone()),
        message: message.to_string(),
    }
}

fn inactive_status(message: &str) -> WeakNetworkStatus {
    WeakNetworkStatus {
        active: false,
        device_id: None,
        target_package: None,
        expires_at: None,
        message: message.to_string(),
    }
}

fn format_millis(value: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(value)
        .map(|time| time.to_rfc3339_opts(SecondsFormat::Secs, true))
}

async fn run_adb_timeout(args: &[&str], seconds: u64) -> Result<String, String> {
    tokio::time::timeout(Duration::from_secs(seconds), process::run_adb_command(args))
        .await
        .map_err(|_| format!("ADB 命令在 {seconds} 秒后超时"))?
        .map_err(|error| error.message)
}

async fn run_shell_args_timeout(
    device_id: &str,
    args: &[&str],
    seconds: u64,
) -> Result<String, String> {
    process::run_shell_command_with_timeout(device_id, args, Duration::from_secs(seconds))
        .await
        .map_err(|error| error.message)
}

fn validate_config(config: &WeakNetworkConfig) -> Result<(), String> {
    validate_package_name(&config.target_package)?;
    if config.upload_kbps > 1_000_000 || config.download_kbps > 1_000_000 {
        return Err("带宽必须在 0 到 1,000,000 Kbps 之间，0 表示不限速".to_string());
    }
    if config.latency_ms > 5_000 || config.jitter_ms > 5_000 {
        return Err("延迟和抖动不能超过 5000 毫秒".to_string());
    }
    if config.jitter_ms > config.latency_ms {
        return Err("延迟抖动不能大于基础延迟".to_string());
    }
    for (label, value) in [
        ("丢包率", config.loss_percent),
        ("重复包率", config.duplicate_percent),
        ("乱序率", config.reorder_percent),
    ] {
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err(format!("{label}必须在 0 到 100 之间"));
        }
    }
    if !(10..=3_600).contains(&config.duration_seconds) {
        return Err("测试时长必须在 10 秒到 3600 秒之间".to_string());
    }
    if config.upload_kbps == 0
        && config.download_kbps == 0
        && config.latency_ms == 0
        && config.loss_percent == 0.0
        && config.duplicate_percent == 0.0
        && config.reorder_percent == 0.0
    {
        return Err("至少设置一个弱网参数".to_string());
    }
    Ok(())
}

fn validate_package_name(package_name: &str) -> Result<(), String> {
    if package_name.len() <= 255
        && package_name.contains('.')
        && package_name != HELPER_PACKAGE
        && package_name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._".contains(character))
    {
        Ok(())
    } else {
        Err("目标应用包名无效".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extract_broadcast_data, parse_helper_status_data, validate_config, validate_package_name,
    };
    use crate::types::WeakNetworkConfig;

    fn config() -> WeakNetworkConfig {
        WeakNetworkConfig {
            target_package: "com.example.game".to_string(),
            upload_kbps: 512,
            download_kbps: 2_048,
            latency_ms: 150,
            jitter_ms: 50,
            loss_percent: 2.0,
            duplicate_percent: 0.5,
            reorder_percent: 1.0,
            duration_seconds: 300,
        }
    }

    #[test]
    fn parses_helper_broadcast_status() {
        let output = "Broadcasting\nBroadcast completed: result=-1, data=\"1|1|1.0.0|com.example.game|1700000000000\"";
        let data = extract_broadcast_data(output).expect("broadcast data");
        let status = parse_helper_status_data(data).expect("helper status");
        assert!(status.running);
        assert!(status.authorized);
        assert_eq!(status.target_package.as_deref(), Some("com.example.game"));
        assert_eq!(status.expires_at_millis, Some(1_700_000_000_000));
    }

    fn helper_status(running: bool) -> super::HelperStatus {
        super::HelperStatus {
            running,
            authorized: true,
            version: super::HELPER_VERSION.to_string(),
            target_package: running.then(|| "com.example.game".to_string()),
            expires_at_millis: None,
        }
    }

    #[test]
    fn transient_control_activity_never_waits_for_launch_completion() {
        for action in [
            super::ACTION_AUTHORIZE,
            super::ACTION_APPLY,
            super::ACTION_STOP,
        ] {
            let args = super::control_activity_args(
                action,
                &[("--es", "target_package", "com.example.game".to_string())],
            );
            assert_eq!(&args[..2], &["am", "start"]);
            assert!(!args.iter().any(|arg| arg == "-W"));
            assert!(args.windows(2).any(|pair| pair == ["-a", action]));
            assert!(args
                .windows(3)
                .any(|parts| parts == ["--es", "target_package", "com.example.game"]));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn stop_command_timeout_is_reconciled_with_actual_vpn_state() {
        let mut statuses =
            std::collections::VecDeque::from([Ok(helper_status(true)), Ok(helper_status(false))]);
        super::confirm_helper_stopped(
            Err("ADB shell command timed out after 10 seconds".into()),
            || std::future::ready(statuses.pop_front().expect("unexpected query")),
        )
        .await
        .expect("confirmed stopped VPN must not fail on activity dispatch timeout");
        assert!(statuses.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn stop_confirmation_retries_transient_status_errors() {
        let mut statuses = std::collections::VecDeque::from([
            Err("temporary broadcast failure".into()),
            Ok(helper_status(true)),
            Ok(helper_status(false)),
        ]);
        super::confirm_helper_stopped(Ok(()), || {
            std::future::ready(statuses.pop_front().expect("unexpected query"))
        })
        .await
        .expect("retry status queries within the deadline");
    }

    #[tokio::test(start_paused = true)]
    async fn successful_dispatch_does_not_claim_a_running_vpn_has_stopped() {
        let started = tokio::time::Instant::now();
        let error =
            super::confirm_helper_stopped(Ok(()), || std::future::ready(Ok(helper_status(true))))
                .await
                .expect_err("dispatch is not confirmation");
        assert!(error.contains("仍在运行"));
        assert_eq!(started.elapsed(), std::time::Duration::from_secs(8));
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_status_never_claims_recovery_and_preserves_request_error() {
        let error = super::confirm_helper_stopped(Err("stop request timeout".into()), || {
            std::future::ready(Err("device offline".into()))
        })
        .await
        .expect_err("offline device cannot confirm recovery");
        assert!(error.contains("stop request timeout"));
        assert!(error.contains("device offline"));
    }

    #[tokio::test(start_paused = true)]
    async fn blocked_stop_status_query_has_a_total_deadline() {
        let started = tokio::time::Instant::now();
        let error = super::confirm_helper_stopped(Ok(()), || {
            std::future::pending::<Result<super::HelperStatus, String>>()
        })
        .await
        .expect_err("blocked query must release cleanup");
        assert!(error.contains("超时"));
        assert_eq!(started.elapsed(), std::time::Duration::from_secs(8));
    }

    #[test]
    fn cleanup_reports_vpn_and_reverse_failures_separately() {
        assert!(super::device_cleanup_result(Ok(()), Ok(())).is_ok());
        let vpn = super::device_cleanup_result(Err("VPN timeout".into()), Ok(())).unwrap_err();
        assert!(vpn.contains("VPN timeout"));
        assert!(vpn.contains("反向映射已清理"));
        assert!(!vpn.contains("Ok(())"));
        let reverse =
            super::device_cleanup_result(Ok(()), Err("reverse offline".into())).unwrap_err();
        assert!(reverse.contains("VPN 已停止"));
        assert!(reverse.contains("reverse offline"));
        let both =
            super::device_cleanup_result(Err("VPN timeout".into()), Err("reverse offline".into()))
                .unwrap_err();
        assert!(both.contains("VPN timeout"));
        assert!(both.contains("reverse offline"));
    }

    #[test]
    fn vpn_release_checks_addresses_instead_of_a_fixed_tun_name() {
        assert!(super::has_helper_vpn_address(
            "40: tun3 inet 10.111.222.1/30 scope global tun3"
        ));
        assert!(super::has_helper_vpn_address(
            "40: tun3 inet6 fd00:111:222::1/126 scope global"
        ));
        assert!(!super::has_helper_vpn_address(
            "7: tun0 inet 10.8.0.1/24 scope global tun0"
        ));
        assert!(!super::has_helper_vpn_address(
            "7: wlan0 inet 10.111.222.10/30 scope global"
        ));
    }

    async fn verify_device_vpn_released(device: &str) -> Result<(), String> {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let interface = super::helper_vpn_interface_present(device).await?;
                let services = super::run_shell_args_timeout(
                    device,
                    &["dumpsys", "activity", "services", super::HELPER_PACKAGE],
                    3,
                )
                .await?;
                let routes = super::run_shell_args_timeout(
                    device,
                    &["ip", "route", "show", "table", "all"],
                    3,
                )
                .await?;
                if !interface
                    && !services.contains("hev.sockstun.TProxyService")
                    && !routes.contains("10.111.222.")
                    && !routes.contains("fd00:111:222:")
                {
                    return Ok::<(), String>(());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| "VPN interface, helper service or routes remained after stop".to_string())?
    }

    // Explicit opt-in: this starts a short per-app VPN on the selected device.
    #[tokio::test]
    #[ignore = "requires ADB_MONSTER_TEST_DEVICE and ADB_MONSTER_TEST_PACKAGE, authorized helper and idle VPN"]
    async fn real_device_weak_network_reapply() {
        let device =
            std::env::var("ADB_MONSTER_TEST_DEVICE").expect("explicit test device required");
        let package =
            std::env::var("ADB_MONSTER_TEST_PACKAGE").expect("explicit test package required");
        let initial = super::query_helper_status(&device)
            .await
            .expect("read helper state");
        assert!(
            initial.authorized && !initial.running,
            "helper must be authorized and idle"
        );
        assert_eq!(initial.version, super::HELPER_VERSION);
        verify_device_vpn_released(&device)
            .await
            .expect("actual VPN resources must be released");
        assert!(
            super::package_installed(&device, &package).await,
            "test package must exist"
        );
        let mapping = format!("tcp:{}", super::DEVICE_PROXY_PORT);
        let reverse = super::run_adb_timeout(&["-s", &device, "reverse", "--list"], 8)
            .await
            .unwrap();
        assert!(
            !reverse.split_whitespace().any(|part| part == mapping),
            "do not replace an existing mapping"
        );

        // Final round exercises the helper emergency timer without desktop stop.
        for (latency, expires_on_device) in [(80, false), (200, false), (500, false), (80, true)] {
            let mut profile = config();
            profile.target_package.clone_from(&package);
            profile.duration_seconds = 30;
            profile.latency_ms = latency;
            let username = "adbmonster".to_string();
            let password = format!(
                "{}{}",
                std::process::id(),
                chrono::Utc::now().timestamp_micros()
            );
            let proxy = crate::network_proxy::start_proxy(
                profile.clone(),
                username.clone(),
                password.clone(),
            )
            .await
            .unwrap();
            let result: Result<(), String> = async {
                super::create_reverse(&device, proxy.port).await?;
                let extras = vec![
                    ("--es", "target_package", package.clone()),
                    ("--es", "proxy_username", username),
                    ("--es", "proxy_password", password),
                    ("--ei", "proxy_port", super::DEVICE_PROXY_PORT.to_string()),
                    (
                        "--ei",
                        "duration_seconds",
                        if expires_on_device {
                            "10".to_string()
                        } else {
                            (profile.duration_seconds + 90).to_string()
                        },
                    ),
                ];
                super::launch_control_activity(&device, super::ACTION_APPLY, &extras).await?;
                tokio::time::timeout(std::time::Duration::from_secs(8), async {
                    loop {
                        let status = super::query_helper_status(&device).await?;
                        if status.running
                            && status.target_package.as_deref() == Some(package.as_str())
                        {
                            return Ok::<(), String>(());
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                })
                .await
                .map_err(|_| "VPN startup confirmation timed out".to_string())??;
                if !super::helper_vpn_interface_present(&device).await? {
                    return Err("VPN startup did not create a real system interface".to_string());
                }
                let services = super::run_shell_args_timeout(
                    &device,
                    &["dumpsys", "activity", "services", super::HELPER_PACKAGE],
                    3,
                )
                .await?;
                if !services.contains("isForeground=true")
                    || !services.contains("android.net.VpnService")
                {
                    return Err("Expected a foreground VPN service bound by Android".to_string());
                }
                if expires_on_device {
                    tokio::time::timeout(std::time::Duration::from_secs(15), async {
                        loop {
                            if !super::query_helper_status(&device).await?.running {
                                return Ok::<(), String>(());
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }
                    })
                    .await
                    .map_err(|_| {
                        "Device expiry failed to close VPN without desktop stop".to_string()
                    })??;
                    verify_device_vpn_released(&device).await?;
                } else {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                Ok(())
            }
            .await;
            // Clean up even if applying the profile failed, before asserting.
            let proxy_cleanup = proxy.stop_and_wait().await;
            let cleanup_started = tokio::time::Instant::now();
            let cleanup = super::cleanup_device(&device).await;
            let cleanup_elapsed = cleanup_started.elapsed();
            assert!(
                result.is_ok(),
                "apply failed: {result:?}; cleanup: {cleanup:?}"
            );
            proxy_cleanup.expect("local proxy cleanup");
            cleanup.expect("VPN and reverse cleanup");
            assert!(
                cleanup_elapsed < std::time::Duration::from_secs(8),
                "cleanup must not wait on activity launch: {cleanup_elapsed:?}"
            );
            assert!(!super::query_helper_status(&device).await.unwrap().running);
            verify_device_vpn_released(&device)
                .await
                .expect("actual VPN resources must be released");
            super::cleanup_device(&device)
                .await
                .expect("repeated stop is idempotent");
            verify_device_vpn_released(&device)
                .await
                .expect("actual VPN resources must be released");
            let reverse = super::run_adb_timeout(&["-s", &device, "reverse", "--list"], 8)
                .await
                .unwrap();
            assert!(!reverse.split_whitespace().any(|part| part == mapping));
            println!(
                "Applied {latency} ms profile (device expiry: {expires_on_device}); VPN interface, service, routes and reverse released; cleanup {cleanup_elapsed:?}"
            );
        }
    }

    #[test]
    fn rejects_unsafe_package_and_config_values() {
        assert!(validate_package_name("com.example.game").is_ok());
        assert!(validate_package_name("com.example;reboot").is_err());
        let mut invalid = config();
        invalid.loss_percent = f32::NAN;
        assert!(validate_config(&invalid).is_err());
        invalid = config();
        invalid.jitter_ms = invalid.latency_ms + 1;
        assert!(validate_config(&invalid).is_err());
    }
}
