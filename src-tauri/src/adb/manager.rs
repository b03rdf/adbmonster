use std::path::PathBuf;
use crate::error::AdbResult;
use crate::types::{Device, RemoteFile};
use super::parser;
use super::process;

pub fn find_adb() -> AdbResult<std::path::PathBuf> {
    if let Ok(path) = which::which("adb") {
        return Ok(path);
    }

    if let Ok(exe) = std::env::current_exe() {
        let bundled = exe.parent().unwrap_or(&exe).join("platform-tools").join("adb.exe");
        if bundled.exists() {
            return Ok(bundled);
        }
    }

    let local = PathBuf::from("platform-tools/adb.exe");
    if local.exists() {
        return Ok(local);
    }

    Err(crate::error::AdbError::new(
        "ADB not found. Please install Android Platform Tools or add adb to PATH.".to_string()
    ))
}

pub async fn list_devices() -> AdbResult<Vec<Device>> {
    let output = process::run_adb_command(&["devices", "-l"]).await?;
    Ok(parser::parse_devices_output(&output))
}

pub async fn get_device_ip(device_id: &str) -> AdbResult<String> {
    let output = process::run_shell_command(device_id, &["ip", "addr", "show"]).await?;
    Ok(parser::parse_ip_output(&output))
}

pub async fn connect_device(ip: &str) -> AdbResult<String> {
    process::run_adb_command(&["connect", ip]).await
}

pub async fn disconnect_device(ip: &str) -> AdbResult<String> {
    process::run_adb_command(&["disconnect", ip]).await
}

pub async fn tcpip(port: u16) -> AdbResult<String> {
    process::run_adb_command(&["tcpip", &port.to_string()]).await
}

pub async fn pair_device(ip: &str, port: u16, code: &str) -> AdbResult<String> {
    let addr = format!("{}:{}", ip, port);
    process::run_adb_command(&["pair", &addr, code]).await
}

pub async fn install_apk(device_id: &str, apk_path: &str) -> AdbResult<String> {
    process::run_adb_command(&["-s", device_id, "install", "-r", apk_path]).await
}

pub async fn uninstall_apk(device_id: &str, package_name: &str) -> AdbResult<String> {
    process::run_adb_command(&["-s", device_id, "uninstall", package_name]).await
}

pub async fn clear_app(device_id: &str, package_name: &str) -> AdbResult<String> {
    process::run_shell_command(device_id, &["pm", "clear", package_name]).await
}

pub async fn get_package_info(device_id: &str, package_name: &str) -> AdbResult<String> {
    process::run_shell_raw(device_id, &format!("dumpsys package {}", package_name)).await
}

pub async fn take_screenshot(device_id: &str) -> AdbResult<Vec<u8>> {
    let adb_path = find_adb()?;
    let output = super::process::prepare_command(&adb_path)
        .args(["-s", device_id, "exec-out", "screencap", "-p"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::error::AdbError::new(stderr.to_string()));
    }

    Ok(output.stdout)
}

pub async fn save_screenshot_to_file(device_id: &str, output_path: &str) -> AdbResult<String> {
    let png_data = take_screenshot(device_id).await?;
    tokio::fs::write(output_path, &png_data).await?;
    Ok(output_path.to_string())
}

#[allow(dead_code)]
pub async fn start_screen_record(device_id: &str, remote_path: &str) -> AdbResult<()> {
    let adb_path = find_adb()?;
    let _child = super::process::prepare_command(&adb_path)
        .args(["-s", device_id, "shell", "screenrecord", remote_path])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    Ok(())
}

pub async fn pull_file(device_id: &str, remote: &str, local: &str) -> AdbResult<String> {
    process::run_adb_command(&["-s", device_id, "pull", remote, local]).await
}

pub async fn pull_clog(device_id: &str, package_name: &str, local_dir: &str) -> AdbResult<String> {
    let remote_path = format!("/sdcard/Android/data/{}/CLog", package_name);
    pull_file(device_id, &remote_path, local_dir).await
}

pub async fn get_apk_path(device_id: &str, package_name: &str) -> AdbResult<String> {
    let output = process::run_shell_command(device_id, &["pm", "path", package_name]).await?;
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(path) = trimmed.strip_prefix("package:") {
            return Ok(path.to_string());
        }
    }
    Err(crate::error::AdbError::new(format!("APK path not found for package: {}", package_name)))
}

pub async fn pull_apk(device_id: &str, package_name: &str, local_path: &str) -> AdbResult<String> {
    let remote_path = get_apk_path(device_id, package_name).await?;
    pull_file(device_id, &remote_path, local_path).await
}

pub async fn list_third_party_packages(device_id: &str) -> AdbResult<Vec<String>> {
    let output = process::run_shell_command(device_id, &["pm", "list", "packages", "-3"]).await?;
    let mut packages = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(pkg) = trimmed.strip_prefix("package:") {
            packages.push(pkg.to_string());
        }
    }
    packages.sort();
    Ok(packages)
}

pub async fn list_remote_files(device_id: &str, path: &str) -> AdbResult<Vec<RemoteFile>> {
    let output = process::run_shell_raw(device_id, &format!("ls -la '{}'", path.replace('\'', "'\"'\"'"))).await?;
    let base = path.trim_end_matches('/');
    let mut files = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("total") {
            continue;
        }
        if let Some(file) = parse_ls_line(line, base) {
            if file.name == "." || file.name == ".." {
                continue;
            }
            files.push(file);
        }
    }

    files.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(files)
}

fn parse_ls_line(line: &str, base_path: &str) -> Option<RemoteFile> {
    if line.is_empty() || line.starts_with("total") {
        return None;
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 6 {
        return None;
    }

    let perms = parts[0];
    let is_dir = perms.starts_with('d');
    let is_symlink = perms.starts_with('l');

    // Locate the size field. Most Android `ls` use the standard format, but some omit
    // the link-count column, so we pick a candidate and fall back to scanning.
    let has_link_count = parts[1].parse::<u64>().is_ok();
    let mut size_idx = if has_link_count { 4 } else { 3 };

    if size_idx >= parts.len() || !is_size_token(parts[size_idx]) {
        for (i, part) in parts.iter().enumerate().skip(2) {
            if is_size_token(part) {
                size_idx = i;
                break;
            }
        }
    }

    if size_idx == 0 || size_idx + 1 >= parts.len() {
        return None;
    }

    let size = parse_size(parts[size_idx]);

    // After size comes the date/time, followed by the file name.
    // Date/time can be 2 columns (YYYY-MM-DD HH:MM) or 3 columns (Mmm DD HH:MM).
    // We consume tokens while they look like date/time components.
    let mut time_end_idx = size_idx;
    for i in (size_idx + 1)..parts.len() {
        if looks_like_date_or_time(parts[i]) {
            time_end_idx = i;
        } else {
            break;
        }
    }

    let name_start_idx = time_end_idx + 1;
    if name_start_idx >= parts.len() {
        return None;
    }

    let time_part = parts[size_idx + 1..=time_end_idx].join(" ");
    let mut name = parts[name_start_idx..].join(" ");

    if is_symlink {
        name = name.split(" -> ").next().unwrap_or(&name).to_string();
    }

    let path = if base_path == "/" {
        format!("/{}", name)
    } else {
        format!("{}/{}", base_path, name)
    };

    Some(RemoteFile {
        name,
        path,
        is_dir,
        size,
        modified: time_part,
    })
}

fn is_size_token(s: &str) -> bool {
    s.parse::<u64>().is_ok() || is_human_size(s)
}

fn is_human_size(s: &str) -> bool {
    if s.len() < 2 {
        return false;
    }
    let (num, unit) = s.split_at(s.len() - 1);
    matches!(unit, "K" | "M" | "G" | "T" | "P" | "B") && num.parse::<f64>().is_ok()
}

fn parse_size(s: &str) -> u64 {
    if let Ok(n) = s.parse::<u64>() {
        return n;
    }
    if s.len() < 2 {
        return 0;
    }
    let (num, unit) = s.split_at(s.len() - 1);
    let n = num.parse::<f64>().unwrap_or(0.0);
    let multiplier: u64 = match unit {
        "K" => 1024,
        "M" => 1024 * 1024,
        "G" => 1024 * 1024 * 1024,
        "T" => 1024u64.pow(4),
        "P" => 1024u64.pow(5),
        _ => 1,
    };
    (n * multiplier as f64) as u64
}

fn looks_like_date_or_time(s: &str) -> bool {
    if s.contains(':') {
        return true;
    }
    // YYYY-MM-DD or YYYY/MM/DD
    if s.len() == 10
        && (s.chars().nth(4) == Some('-') || s.chars().nth(4) == Some('/'))
        && (s.chars().nth(7) == Some('-') || s.chars().nth(7) == Some('/'))
    {
        return true;
    }
    // Month abbreviation
    const MONTHS: &[&str] = &[
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    if MONTHS.contains(&s) {
        return true;
    }
    // Day number in a date
    if let Ok(d) = s.parse::<u32>() {
        if (1..=31).contains(&d) {
            return true;
        }
    }
    false
}

pub async fn push_file(device_id: &str, local: &str, remote: &str) -> AdbResult<String> {
    process::run_adb_command(&["-s", device_id, "push", local, remote]).await
}
