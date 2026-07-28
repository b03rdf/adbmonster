use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use zip::write::SimpleFileOptions;

use crate::adb::{manager, process};
use crate::types::{DeviceMetrics, DiagnosticProgress, DiagnosticStatus};

#[derive(Default)]
pub struct DiagnosticState {
    running: AtomicBool,
    progress: Mutex<Option<DiagnosticProgress>>,
}

#[tauri::command]
pub async fn get_device_metrics(device_id: String) -> Result<DeviceMetrics, String> {
    let command = process::run_shell_raw(
        &device_id,
        "echo __BATTERY__; dumpsys battery; echo __MEMORY__; cat /proc/meminfo; \
         echo __STORAGE__; df -k /data; echo __CPU__; dumpsys cpuinfo; \
         echo __ACTIVITY__; dumpsys activity activities; echo __UPTIME__; cat /proc/uptime",
    );
    let output = tokio::time::timeout(Duration::from_secs(20), command)
        .await
        .map_err(|_| "Device metrics collection timed out after 20 seconds".to_string())?
        .map_err(|error| error.message)?;

    Ok(parse_device_metrics(
        extract_section(&output, "__BATTERY__", "__MEMORY__"),
        extract_section(&output, "__MEMORY__", "__STORAGE__"),
        extract_section(&output, "__STORAGE__", "__CPU__"),
        extract_section(&output, "__CPU__", "__ACTIVITY__"),
        extract_section(&output, "__ACTIVITY__", "__UPTIME__"),
        extract_section(&output, "__UPTIME__", ""),
    ))
}

#[tauri::command]
pub async fn get_diagnostic_status(app: AppHandle) -> Result<DiagnosticStatus, String> {
    let state = app.state::<DiagnosticState>();
    let progress = state
        .progress
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    Ok(DiagnosticStatus {
        running: state.running.load(Ordering::Acquire),
        progress,
    })
}

#[tauri::command]
pub async fn create_diagnostic_package(
    device_id: String,
    package_name: Option<String>,
    output_path: String,
    include_bugreport: bool,
    app: AppHandle,
) -> Result<String, String> {
    if output_path.trim().is_empty() {
        return Err("Output path must not be empty".to_string());
    }
    if let Some(package) = package_name.as_deref().filter(|value| !value.is_empty()) {
        manager::validate_package_name(package).map_err(|error| error.message)?;
    }

    let state = app.state::<DiagnosticState>();
    if state
        .running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("A diagnostic package is already being generated".to_string());
    }

    let temp_dir = diagnostic_temp_dir(&device_id);
    let result = async {
        emit_progress(&app, "prepare", "正在准备诊断目录", 3);
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|error| error.to_string())?;
        collect_diagnostic_files(
            &device_id,
            package_name.as_deref().filter(|value| !value.is_empty()),
            &output_path,
            include_bugreport,
            &temp_dir,
            &app,
        )
        .await
    }
    .await;

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    state.running.store(false, Ordering::Release);
    if let Err(error) = &result {
        emit_progress(&app, "error", &format!("诊断包生成失败：{error}"), 0);
    }
    result
}

async fn collect_diagnostic_files(
    device_id: &str,
    package_name: Option<&str>,
    output_path: &str,
    include_bugreport: bool,
    temp_dir: &Path,
    app: &AppHandle,
) -> Result<String, String> {
    emit_progress(app, "device", "正在收集设备基础信息", 10);
    let properties = capture_or_error(device_id, &["getprop"]).await;
    write_text(temp_dir.join("device-properties.txt"), &properties).await?;

    let metrics = get_device_metrics(device_id.to_string()).await?;
    let summary = json!({
        "generatedAt": Utc::now().to_rfc3339(),
        "deviceId": device_id,
        "packageName": package_name,
        "metrics": metrics,
    });
    write_text(
        temp_dir.join("summary.json"),
        &serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?,
    )
    .await?;

    emit_progress(app, "system", "正在收集系统、电量和存储信息", 22);
    let system = capture_or_error(device_id, &["dumpsys", "battery"]).await;
    write_text(temp_dir.join("battery.txt"), &system).await?;
    write_text(
        temp_dir.join("storage.txt"),
        &capture_or_error(device_id, &["df", "-k"]).await,
    )
    .await?;
    write_text(
        temp_dir.join("processes.txt"),
        &capture_or_error(device_id, &["ps", "-A"]).await,
    )
    .await?;

    emit_progress(app, "logcat", "正在导出最近日志", 36);
    write_text(
        temp_dir.join("logcat.txt"),
        &capture_or_error(
            device_id,
            &["logcat", "-d", "-v", "threadtime", "-t", "5000"],
        )
        .await,
    )
    .await?;

    if let Some(package) = package_name {
        emit_progress(app, "application", "正在收集应用与性能信息", 50);
        write_text(
            temp_dir.join("package-info.txt"),
            &capture_or_error(device_id, &["dumpsys", "package", package]).await,
        )
        .await?;
        write_text(
            temp_dir.join("meminfo.txt"),
            &capture_or_error(device_id, &["dumpsys", "meminfo", package]).await,
        )
        .await?;
        write_text(
            temp_dir.join("gfxinfo-framestats.txt"),
            &capture_or_error(device_id, &["dumpsys", "gfxinfo", package, "framestats"]).await,
        )
        .await?;
    }

    emit_progress(app, "screenshot", "正在截取设备屏幕", 62);
    match manager::take_screenshot(device_id).await {
        Ok(data) => tokio::fs::write(temp_dir.join("screenshot.png"), data)
            .await
            .map_err(|error| error.to_string())?,
        Err(error) => write_text(temp_dir.join("screenshot-error.txt"), &error.message).await?,
    }

    if include_bugreport {
        emit_progress(
            app,
            "bugreport",
            "正在生成完整 Bugreport，可能需要数分钟",
            70,
        );
        capture_bugreport(device_id, &temp_dir.join("bugreport.zip")).await;
    }

    emit_progress(app, "archive", "正在压缩诊断文件", 92);
    let source = temp_dir.to_path_buf();
    let destination = PathBuf::from(output_path);
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    let destination_for_zip = destination.clone();
    let archive_result =
        tokio::task::spawn_blocking(move || create_zip(&source, &destination_for_zip))
            .await
            .map_err(|error| error.to_string())?;
    if let Err(error) = archive_result {
        let _ = tokio::fs::remove_file(&destination).await;
        return Err(error);
    }

    emit_progress(app, "complete", "诊断包生成完成", 100);
    Ok(destination.to_string_lossy().to_string())
}

async fn capture_shell(device_id: &str, args: &[&str]) -> Result<String, String> {
    tokio::time::timeout(
        Duration::from_secs(30),
        process::run_shell_command(device_id, args),
    )
    .await
    .map_err(|_| format!("Command timed out: {}", args.join(" ")))?
    .map_err(|error| error.message)
}

async fn capture_or_error(device_id: &str, args: &[&str]) -> String {
    capture_shell(device_id, args)
        .await
        .unwrap_or_else(|error| format!("Command failed: {error}"))
}

async fn capture_bugreport(device_id: &str, destination: &Path) {
    let result = async {
        let adb_path = manager::find_adb().map_err(|error| error.message)?;
        let destination_text = destination.to_string_lossy().to_string();
        let output = process::prepare_command(&adb_path)
            .args(["-s", device_id, "bugreport", &destination_text])
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    };

    let error = match tokio::time::timeout(Duration::from_secs(300), result).await {
        Ok(Ok(())) => return,
        Ok(Err(error)) => error,
        Err(_) => "Bugreport timed out after 5 minutes".to_string(),
    };
    let _ = write_text(destination.with_file_name("bugreport-error.txt"), &error).await;
}

async fn write_text(path: PathBuf, contents: &str) -> Result<(), String> {
    tokio::fs::write(path, contents)
        .await
        .map_err(|error| error.to_string())
}

fn create_zip(source_dir: &Path, destination: &Path) -> Result<(), String> {
    let file = File::create(destination).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipWriter::new(file);
    let entries = std::fs::read_dir(source_dir).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let compression = if name.ends_with(".zip") || name.ends_with(".png") {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        let options = SimpleFileOptions::default()
            .compression_method(compression)
            .unix_permissions(0o644);
        archive
            .start_file(name, options)
            .map_err(|error| error.to_string())?;
        let mut source = File::open(entry.path()).map_err(|error| error.to_string())?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = source
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            archive
                .write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
        }
    }
    archive.finish().map_err(|error| error.to_string())?;
    Ok(())
}

fn diagnostic_temp_dir(device_id: &str) -> PathBuf {
    let safe_device: String = device_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect();
    std::env::temp_dir().join(format!(
        "adb-monster-diagnostic-{}-{}",
        safe_device,
        Utc::now().timestamp_millis()
    ))
}

fn emit_progress(app: &AppHandle, stage: &str, message: &str, percent: u8) {
    let progress = DiagnosticProgress {
        stage: stage.to_string(),
        message: message.to_string(),
        percent,
    };
    if let Ok(mut current) = app.state::<DiagnosticState>().progress.lock() {
        *current = Some(progress.clone());
    }
    let _ = app.emit("diagnostic:progress", progress);
}

fn extract_section<'a>(input: &'a str, start: &str, end: &str) -> &'a str {
    let Some((_, remainder)) = input.split_once(start) else {
        return "";
    };
    if end.is_empty() {
        remainder
    } else {
        remainder
            .split_once(end)
            .map_or(remainder, |(section, _)| section)
    }
}

fn parse_device_metrics(
    battery: &str,
    memory: &str,
    storage: &str,
    cpu: &str,
    activity: &str,
    uptime: &str,
) -> DeviceMetrics {
    let battery_level = parse_key_u64(battery, "level").and_then(|value| u8::try_from(value).ok());
    let battery_temperature_c =
        parse_key_u64(battery, "temperature").map(|value| value as f32 / 10.0);
    let charging = [
        "AC powered: true",
        "USB powered: true",
        "Wireless powered: true",
    ]
    .iter()
    .any(|needle| battery.contains(needle));

    let memory_total_kb = parse_meminfo(memory, "MemTotal").unwrap_or(0);
    let memory_available_kb = parse_meminfo(memory, "MemAvailable")
        .or_else(|| parse_meminfo(memory, "MemFree"))
        .unwrap_or(0);
    let (storage_total_kb, storage_available_kb) = parse_storage(storage);

    DeviceMetrics {
        battery_level,
        battery_temperature_c,
        charging,
        memory_total_kb,
        memory_available_kb,
        storage_total_kb,
        storage_available_kb,
        cpu_usage_percent: parse_cpu_total(cpu),
        uptime_seconds: uptime
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0) as u64,
        foreground_activity: parse_foreground_activity(activity),
        collected_at: Utc::now().to_rfc3339(),
    }
}

fn parse_key_u64(input: &str, key: &str) -> Option<u64> {
    input.lines().find_map(|line| {
        let (candidate, value) = line.trim().split_once(':')?;
        (candidate.trim() == key)
            .then(|| value.trim().parse::<u64>().ok())
            .flatten()
    })
}

fn parse_meminfo(input: &str, key: &str) -> Option<u64> {
    input.lines().find_map(|line| {
        let (candidate, value) = line.split_once(':')?;
        (candidate.trim() == key)
            .then(|| value.split_whitespace().next()?.parse::<u64>().ok())
            .flatten()
    })
}

fn parse_storage(input: &str) -> (u64, u64) {
    input
        .lines()
        .rev()
        .find_map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            if columns.len() < 6 || !columns.last()?.starts_with("/data") {
                return None;
            }
            Some((columns[1].parse().ok()?, columns[3].parse().ok()?))
        })
        .unwrap_or((0, 0))
}

fn parse_cpu_total(input: &str) -> Option<f32> {
    input.lines().find_map(|line| {
        let total = line.trim().split_once("% TOTAL")?.0.trim();
        total.parse::<f32>().ok()
    })
}

fn parse_foreground_activity(input: &str) -> String {
    input
        .lines()
        .find(|line| line.contains("mResumedActivity") || line.contains("topResumedActivity"))
        .and_then(|line| line.split_whitespace().find(|part| part.contains('/')))
        .unwrap_or("Unknown")
        .trim_end_matches('}')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        create_zip, extract_section, parse_cpu_total, parse_device_metrics,
        parse_foreground_activity, parse_storage,
    };

    #[test]
    fn parses_dashboard_metrics() {
        let metrics = parse_device_metrics(
            "AC powered: false\nUSB powered: true\nlevel: 76\ntemperature: 315",
            "MemTotal: 8000000 kB\nMemAvailable: 3000000 kB",
            "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/block/dm-1 100000 40000 60000 40% /data",
            "12.5% TOTAL: 8% user + 4.5% kernel",
            "mResumedActivity: ActivityRecord{abc u0 com.example/.MainActivity t1}",
            "1234.50 22.0",
        );
        assert_eq!(metrics.battery_level, Some(76));
        assert_eq!(metrics.battery_temperature_c, Some(31.5));
        assert!(metrics.charging);
        assert_eq!(metrics.memory_available_kb, 3_000_000);
        assert_eq!(metrics.storage_available_kb, 60_000);
        assert_eq!(metrics.cpu_usage_percent, Some(12.5));
        assert_eq!(metrics.uptime_seconds, 1234);
        assert_eq!(metrics.foreground_activity, "com.example/.MainActivity");
    }

    #[test]
    fn parses_common_metric_variants() {
        assert_eq!(parse_storage("/dev/x 200 50 150 25% /data"), (200, 150));
        assert_eq!(parse_cpu_total(" 7% TOTAL: 3% user"), Some(7.0));
        assert_eq!(
            parse_foreground_activity("topResumedActivity=ActivityRecord{1 u0 a.b/.Home t3}"),
            "a.b/.Home"
        );
        assert_eq!(
            extract_section("__ONE__alpha__TWO__beta", "__ONE__", "__TWO__"),
            "alpha"
        );
    }

    #[test]
    fn creates_diagnostic_zip() {
        let root =
            std::env::temp_dir().join(format!("adb-monster-zip-test-{}", std::process::id()));
        let source = root.join("source");
        let destination = root.join("diagnostic.zip");
        std::fs::create_dir_all(&source).expect("create source directory");
        std::fs::write(source.join("summary.json"), "{\"ok\":true}").expect("write source file");

        create_zip(&source, &destination).expect("create zip archive");
        assert!(std::fs::metadata(&destination).expect("zip metadata").len() > 0);

        let _ = std::fs::remove_dir_all(root);
    }
}
