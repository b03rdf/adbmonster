use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use tauri::{AppHandle, Emitter, Manager};

use crate::adb::process;
use crate::types::{WeakNetworkCapabilities, WeakNetworkConfig, WeakNetworkStatus};

const MODE_EMULATOR: &str = "android_emulator";
const MODE_ROOT_NETEM: &str = "root_netem";

#[derive(Debug, Clone)]
struct WeakNetworkSession {
    id: u64,
    device_id: String,
    mode: String,
    interface_name: Option<String>,
    expires_at: String,
}

#[derive(Default)]
pub struct WeakNetworkState {
    next_id: AtomicU64,
    session: Mutex<Option<WeakNetworkSession>>,
}

#[tauri::command]
pub async fn detect_weak_network_capabilities(
    device_id: String,
) -> Result<WeakNetworkCapabilities, String> {
    detect_capabilities(&device_id).await
}

#[tauri::command]
pub async fn apply_weak_network(
    device_id: String,
    config: WeakNetworkConfig,
    app: AppHandle,
) -> Result<WeakNetworkStatus, String> {
    validate_config(&config)?;
    let capabilities = detect_capabilities(&device_id).await?;
    if !capabilities.supported {
        return Err(capabilities.message);
    }

    let previous = {
        let state = app.state::<WeakNetworkState>();
        let previous = state
            .session
            .lock()
            .map_err(|error| error.to_string())?
            .take();
        previous
    };
    if let Some(previous) = previous {
        if let Err(error) = restore_session(&previous).await {
            let state = app.state::<WeakNetworkState>();
            *state.session.lock().map_err(|reason| reason.to_string())? = Some(previous);
            return Err(error);
        }
    }

    match capabilities.mode.as_str() {
        MODE_EMULATOR => apply_emulator_profile(&device_id, &config).await?,
        MODE_ROOT_NETEM => {
            let interface_name = capabilities
                .interface_name
                .as_deref()
                .ok_or_else(|| "未找到设备默认网络接口".to_string())?;
            apply_netem_profile(&device_id, interface_name, &config).await?;
        }
        _ => return Err("设备不支持当前弱网控制方式".to_string()),
    }

    let id = app
        .state::<WeakNetworkState>()
        .next_id
        .fetch_add(1, Ordering::AcqRel)
        + 1;
    let expires_at = Utc::now()
        .checked_add_signed(chrono::Duration::seconds(config.duration_seconds as i64))
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let session = WeakNetworkSession {
        id,
        device_id: device_id.clone(),
        mode: capabilities.mode.clone(),
        interface_name: capabilities.interface_name.clone(),
        expires_at: expires_at.clone(),
    };
    {
        let state = app.state::<WeakNetworkState>();
        *state.session.lock().map_err(|error| error.to_string())? = Some(session.clone());
    }

    schedule_automatic_restore(app.clone(), session, config.duration_seconds);
    let skipped = capabilities.mode == MODE_ROOT_NETEM && config.download_kbps > 0;
    let status = WeakNetworkStatus {
        active: true,
        device_id: Some(device_id),
        mode: Some(capabilities.mode),
        interface_name: capabilities.interface_name,
        expires_at: Some(expires_at),
        message: if skipped {
            "弱网已生效；Root NetEm模式仅限制设备出口带宽，下行限速需要VPN/IFB支持".to_string()
        } else {
            "弱网配置已生效，将在到期后自动恢复".to_string()
        },
    };
    let _ = app.emit("weak-network:status", status.clone());
    Ok(status)
}

#[tauri::command]
pub async fn stop_weak_network(app: AppHandle) -> Result<WeakNetworkStatus, String> {
    let session = {
        let state = app.state::<WeakNetworkState>();
        let session = state
            .session
            .lock()
            .map_err(|error| error.to_string())?
            .take();
        session
    };
    if let Some(session) = session {
        if let Err(error) = restore_session(&session).await {
            let state = app.state::<WeakNetworkState>();
            *state.session.lock().map_err(|reason| reason.to_string())? = Some(session);
            return Err(error);
        }
    }
    let status = inactive_status("网络已恢复为正常状态");
    let _ = app.emit("weak-network:status", status.clone());
    Ok(status)
}

#[tauri::command]
pub async fn force_restore_weak_network(
    device_id: String,
    app: AppHandle,
) -> Result<WeakNetworkStatus, String> {
    let active_session = {
        let state = app.state::<WeakNetworkState>();
        let mut session = state.session.lock().map_err(|error| error.to_string())?;
        if session
            .as_ref()
            .is_some_and(|current| current.device_id == device_id)
        {
            session.take()
        } else {
            None
        }
    };

    if let Some(session) = active_session {
        if let Err(error) = restore_session(&session).await {
            let state = app.state::<WeakNetworkState>();
            *state.session.lock().map_err(|reason| reason.to_string())? = Some(session);
            return Err(error);
        }
    } else {
        let capabilities = detect_capabilities(&device_id).await?;
        match capabilities.mode.as_str() {
            MODE_EMULATOR => restore_emulator(&device_id).await?,
            MODE_ROOT_NETEM => {
                let interface_name = capabilities
                    .interface_name
                    .as_deref()
                    .ok_or_else(|| "未找到设备默认网络接口".to_string())?;
                restore_netem(&device_id, interface_name).await?;
            }
            _ => return Err(capabilities.message),
        }
    }

    let mut status = get_weak_network_status(app.clone()).await?;
    status.message = if status.active {
        format!(
            "已恢复当前设备；设备 {} 的弱网测试仍在运行",
            status.device_id.as_deref().unwrap_or("未知")
        )
    } else {
        "已强制清除当前设备的弱网规则".to_string()
    };
    let _ = app.emit("weak-network:status", status.clone());
    Ok(status)
}

#[tauri::command]
pub async fn get_weak_network_status(app: AppHandle) -> Result<WeakNetworkStatus, String> {
    let state = app.state::<WeakNetworkState>();
    let session = state
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    Ok(session.map_or_else(
        || inactive_status("当前未启用弱网"),
        |session| WeakNetworkStatus {
            active: true,
            device_id: Some(session.device_id),
            mode: Some(session.mode),
            interface_name: session.interface_name,
            expires_at: Some(session.expires_at),
            message: "弱网配置正在运行".to_string(),
        },
    ))
}

async fn detect_capabilities(device_id: &str) -> Result<WeakNetworkCapabilities, String> {
    if device_id.starts_with("emulator-") && emulator_console_available(device_id).await {
        return Ok(WeakNetworkCapabilities {
            mode: MODE_EMULATOR.to_string(),
            supported: true,
            interface_name: None,
            supports_bandwidth: true,
            supports_downlink: true,
            supports_packet_effects: false,
            message: "标准Android Emulator：支持上下行带宽、延迟和抖动".to_string(),
        });
    }

    if root_netem_available(device_id).await {
        let interface_name = detect_default_interface(device_id).await;
        let supported = interface_name.is_some();
        return Ok(WeakNetworkCapabilities {
            mode: MODE_ROOT_NETEM.to_string(),
            supported,
            interface_name,
            supports_bandwidth: true,
            supports_downlink: false,
            supports_packet_effects: true,
            message: if supported {
                "Root NetEm：支持出口限速、延迟、抖动、丢包、重复包和乱序".to_string()
            } else {
                "检测到Root和tc，但未找到默认网络接口".to_string()
            },
        });
    }

    Ok(WeakNetworkCapabilities {
        mode: "unsupported".to_string(),
        supported: false,
        interface_name: None,
        supports_bandwidth: false,
        supports_downlink: false,
        supports_packet_effects: false,
        message: "该设备不是标准AVD且没有可用的Root tc/netem；非Root真机需要VPN辅助应用"
            .to_string(),
    })
}

async fn emulator_console_available(device_id: &str) -> bool {
    run_adb_timeout(&["-s", device_id, "emu", "network", "status"], 5)
        .await
        .is_ok()
}

async fn root_netem_available(device_id: &str) -> bool {
    let root = run_shell_timeout(device_id, "su -c id", 5).await;
    if !root.is_ok_and(|output| output.contains("uid=0")) {
        return false;
    }
    run_shell_timeout(device_id, "su -c \"tc -V\"", 5)
        .await
        .is_ok()
}

async fn detect_default_interface(device_id: &str) -> Option<String> {
    let output = run_shell_timeout(device_id, "ip route show default", 5)
        .await
        .ok()?;
    parse_default_interface(&output)
}

async fn apply_emulator_profile(device_id: &str, config: &WeakNetworkConfig) -> Result<(), String> {
    let speed = emulator_speed(config.upload_kbps, config.download_kbps);
    let delay = emulator_delay(config.latency_ms, config.jitter_ms);
    if let Err(error) =
        run_adb_timeout(&["-s", device_id, "emu", "network", "speed", &speed], 8).await
    {
        return Err(format!("设置模拟器带宽失败：{error}"));
    }
    if let Err(error) =
        run_adb_timeout(&["-s", device_id, "emu", "network", "delay", &delay], 8).await
    {
        let _ = restore_emulator(device_id).await;
        return Err(format!("设置模拟器延迟失败：{error}"));
    }
    Ok(())
}

async fn apply_netem_profile(
    device_id: &str,
    interface_name: &str,
    config: &WeakNetworkConfig,
) -> Result<(), String> {
    let command = build_netem_command(interface_name, config)?;
    run_shell_timeout(device_id, &format!("su -c \"{command}\""), 10)
        .await
        .map(|_| ())
        .map_err(|error| format!("应用NetEm规则失败：{error}"))
}

fn schedule_automatic_restore(app: AppHandle, session: WeakNetworkSession, seconds: u32) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(seconds as u64)).await;
        let should_restore = {
            let state = app.state::<WeakNetworkState>();
            let Ok(mut current) = state.session.lock() else {
                return;
            };
            if current
                .as_ref()
                .is_some_and(|active| active.id == session.id)
            {
                current.take();
                true
            } else {
                false
            }
        };
        if should_restore {
            let status = match restore_session(&session).await {
                Ok(()) => inactive_status("弱网测试到期，网络已自动恢复"),
                Err(error) => inactive_status(&format!("自动恢复失败，请使用强制恢复：{error}")),
            };
            let _ = app.emit("weak-network:status", status);
        }
    });
}

async fn restore_session(session: &WeakNetworkSession) -> Result<(), String> {
    if session.mode == MODE_EMULATOR {
        restore_emulator(&session.device_id).await
    } else if session.mode == MODE_ROOT_NETEM {
        if let Some(interface_name) = session.interface_name.as_deref() {
            restore_netem(&session.device_id, interface_name).await
        } else {
            Err("缺少应用弱网时使用的网络接口".to_string())
        }
    } else {
        Err("未知的弱网控制模式".to_string())
    }
}

async fn restore_netem(device_id: &str, interface_name: &str) -> Result<(), String> {
    validate_interface(interface_name)?;
    let show_command = format!("su -c \"tc qdisc show dev {interface_name}\"");
    let current = run_shell_timeout(device_id, &show_command, 8).await?;
    if !current.contains("qdisc netem") {
        return Ok(());
    }
    let delete_command = format!("su -c \"tc qdisc del dev {interface_name} root\"");
    run_shell_timeout(device_id, &delete_command, 8)
        .await
        .map(|_| ())
        .map_err(|error| format!("清除 NetEm 规则失败：{error}"))
}

async fn restore_emulator(device_id: &str) -> Result<(), String> {
    let speed = run_adb_timeout(&["-s", device_id, "emu", "network", "speed", "full"], 8).await;
    let delay = run_adb_timeout(&["-s", device_id, "emu", "network", "delay", "none"], 8).await;
    speed.and(delay).map(|_| ())
}

async fn run_adb_timeout(args: &[&str], seconds: u64) -> Result<String, String> {
    tokio::time::timeout(Duration::from_secs(seconds), process::run_adb_command(args))
        .await
        .map_err(|_| format!("ADB command timed out after {seconds} seconds"))?
        .map_err(|error| error.message)
}

async fn run_shell_timeout(device_id: &str, command: &str, seconds: u64) -> Result<String, String> {
    tokio::time::timeout(
        Duration::from_secs(seconds),
        process::run_shell_raw(device_id, command),
    )
    .await
    .map_err(|_| format!("Shell command timed out after {seconds} seconds"))?
    .map_err(|error| error.message)
}

fn validate_config(config: &WeakNetworkConfig) -> Result<(), String> {
    if config.upload_kbps > 1_000_000 || config.download_kbps > 1_000_000 {
        return Err("带宽必须在0到1,000,000 Kbps之间，0表示不限速".to_string());
    }
    if config.latency_ms > 5_000 || config.jitter_ms > 5_000 {
        return Err("延迟和抖动不能超过5000毫秒".to_string());
    }
    if config.jitter_ms > 0 && config.latency_ms == 0 {
        return Err("设置抖动时，基础延迟必须大于0".to_string());
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
            return Err(format!("{label}必须在0到100之间"));
        }
    }
    if config.reorder_percent > 0.0 && config.latency_ms == 0 {
        return Err("设置乱序时，基础延迟必须大于0".to_string());
    }
    if !(10..=3_600).contains(&config.duration_seconds) {
        return Err("测试时长必须在10秒到3600秒之间".to_string());
    }
    Ok(())
}

fn build_netem_command(interface_name: &str, config: &WeakNetworkConfig) -> Result<String, String> {
    validate_interface(interface_name)?;
    validate_config(config)?;
    let mut parts = vec![format!("tc qdisc replace dev {interface_name} root netem")];
    if config.latency_ms > 0 {
        let mut delay = format!("delay {}ms", config.latency_ms);
        if config.jitter_ms > 0 {
            delay.push_str(&format!(" {}ms distribution normal", config.jitter_ms));
        }
        parts.push(delay);
    }
    if config.loss_percent > 0.0 {
        parts.push(format!("loss random {:.2}%", config.loss_percent));
    }
    if config.duplicate_percent > 0.0 {
        parts.push(format!("duplicate {:.2}%", config.duplicate_percent));
    }
    if config.reorder_percent > 0.0 {
        parts.push(format!("reorder {:.2}% 50%", config.reorder_percent));
    }
    if config.upload_kbps > 0 {
        parts.push(format!("rate {}kbit", config.upload_kbps));
    }
    if parts.len() == 1 {
        return Err("至少设置一个弱网参数".to_string());
    }
    Ok(parts.join(" "))
}

fn emulator_speed(upload_kbps: u32, download_kbps: u32) -> String {
    match (upload_kbps, download_kbps) {
        (0, 0) => "full".to_string(),
        (0, down) => format!("{down}:{down}"),
        (up, 0) => format!("{up}:{up}"),
        (up, down) => format!("{up}:{down}"),
    }
}

fn emulator_delay(latency_ms: u32, jitter_ms: u32) -> String {
    if latency_ms == 0 {
        "none".to_string()
    } else if jitter_ms == 0 {
        latency_ms.to_string()
    } else {
        format!(
            "{}:{}",
            latency_ms.saturating_sub(jitter_ms),
            latency_ms.saturating_add(jitter_ms)
        )
    }
}

fn parse_default_interface(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let index = parts.iter().position(|part| *part == "dev")?;
        let interface_name = *parts.get(index + 1)?;
        validate_interface(interface_name)
            .is_ok()
            .then(|| interface_name.to_string())
    })
}

fn validate_interface(interface_name: &str) -> Result<(), String> {
    if !interface_name.is_empty()
        && interface_name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character))
    {
        Ok(())
    } else {
        Err("网络接口名称无效".to_string())
    }
}

fn inactive_status(message: &str) -> WeakNetworkStatus {
    WeakNetworkStatus {
        active: false,
        device_id: None,
        mode: None,
        interface_name: None,
        expires_at: None,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_netem_command, emulator_delay, emulator_speed, parse_default_interface,
        validate_config,
    };
    use crate::types::WeakNetworkConfig;

    fn config() -> WeakNetworkConfig {
        WeakNetworkConfig {
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
    fn builds_safe_netem_command() {
        let command = build_netem_command("wlan0", &config()).expect("valid command");
        assert!(command.contains("delay 150ms 50ms distribution normal"));
        assert!(command.contains("loss random 2.00%"));
        assert!(command.contains("rate 512kbit"));
        assert!(build_netem_command("wlan0; reboot", &config()).is_err());
    }

    #[test]
    fn parses_interface_and_emulator_values() {
        assert_eq!(
            parse_default_interface("default via 10.0.2.2 dev wlan0 proto dhcp"),
            Some("wlan0".to_string())
        );
        assert_eq!(emulator_speed(512, 2_048), "512:2048");
        assert_eq!(emulator_speed(0, 0), "full");
        assert_eq!(emulator_delay(150, 50), "100:200");
        assert_eq!(emulator_delay(0, 0), "none");
    }

    #[test]
    fn rejects_unsafe_weak_network_values() {
        let mut invalid = config();
        invalid.duration_seconds = 0;
        assert!(validate_config(&invalid).is_err());
        invalid = config();
        invalid.loss_percent = f32::NAN;
        assert!(validate_config(&invalid).is_err());
        invalid = config();
        invalid.latency_ms = 0;
        assert!(validate_config(&invalid).is_err());
    }
}
