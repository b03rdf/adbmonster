use std::path::PathBuf;
use crate::error::AdbResult;
use crate::types::Device;
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
