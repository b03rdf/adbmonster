use crate::adb::manager;
use crate::types::AutoConnectResult;
use crate::types::Device;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

#[tauri::command]
pub async fn get_devices() -> Result<Vec<Device>, String> {
    manager::list_devices().await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn connect_device(ip: String) -> Result<String, String> {
    manager::connect_device(&ip).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn disconnect_device(ip: String) -> Result<String, String> {
    manager::disconnect_device(&ip).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn get_device_ip(device_id: String) -> Result<String, String> {
    manager::get_device_ip(&device_id)
        .await
        .map_err(|e| e.message)
}

#[tauri::command]
pub async fn tcpip_connect(port: u16) -> Result<String, String> {
    manager::tcpip(port).await.map_err(|e| e.message)
}

#[tauri::command]
pub async fn pair_device(ip: String, port: u16, code: String) -> Result<String, String> {
    manager::pair_device(&ip, port, &code)
        .await
        .map_err(|e| e.message)
}

#[tauri::command]
pub async fn pair_then_connect(
    ip: String,
    pair_port: u16,
    connect_port: u16,
    code: String,
) -> Result<String, String> {
    let pair_result = manager::pair_device(&ip, pair_port, &code)
        .await
        .map_err(|e| e.message)?;
    let connect_result = manager::connect_device(&format!("{}:{}", ip, connect_port))
        .await
        .map_err(|e| e.message)?;
    Ok(format!("{}; {}", pair_result, connect_result))
}

#[tauri::command]
pub async fn auto_connect_local_emulator(address: String) -> Result<AutoConnectResult, String> {
    let socket = validate_local_emulator_address(&address)?;
    let probe = tokio::task::spawn_blocking(move || {
        TcpStream::connect_timeout(&socket, Duration::from_millis(700)).is_ok()
    })
    .await
    .map_err(|error| error.to_string())?;

    if !probe {
        return Ok(AutoConnectResult {
            connected: false,
            address,
            message: "本地模拟器端口未开放".to_string(),
        });
    }

    let output = tokio::time::timeout(Duration::from_secs(8), manager::connect_device(&address))
        .await
        .map_err(|_| "ADB emulator connection timed out after 8 seconds".to_string())?
        .map_err(|error| error.message)?;
    let normalized = output.to_ascii_lowercase();
    let connected = normalized.contains("connected to") || normalized.contains("already connected");

    Ok(AutoConnectResult {
        connected,
        address,
        message: if output.trim().is_empty() {
            if connected {
                "模拟器连接成功".to_string()
            } else {
                "ADB未返回连接结果".to_string()
            }
        } else {
            output.trim().to_string()
        },
    })
}

fn validate_local_emulator_address(address: &str) -> Result<SocketAddr, String> {
    let socket = address
        .parse::<SocketAddr>()
        .map_err(|_| "模拟器地址格式无效，请使用 127.0.0.1:端口".to_string())?;
    if !socket.ip().is_loopback() {
        return Err("自动模拟器连接仅允许本机回环地址".to_string());
    }
    Ok(socket)
}

#[cfg(test)]
mod tests {
    use super::validate_local_emulator_address;

    #[test]
    fn only_accepts_loopback_emulator_addresses() {
        assert!(validate_local_emulator_address("127.0.0.1:7555").is_ok());
        assert!(validate_local_emulator_address("[::1]:7555").is_ok());
        assert!(validate_local_emulator_address("192.168.1.10:5555").is_err());
        assert!(validate_local_emulator_address("localhost:7555").is_err());
        assert!(validate_local_emulator_address("invalid").is_err());
    }
}
