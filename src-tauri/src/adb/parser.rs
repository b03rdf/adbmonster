use serde::Serialize;

use crate::types::Device;

pub fn parse_devices_output(output: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut in_header = true;

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if in_header {
            if line.starts_with("List of devices") {
                in_header = false;
            }
            continue;
        }

        if let Some(device) = parse_device_line(line) {
            if seen.insert(device.id.clone()) {
                devices.push(device);
            }
        }
    }

    devices
}

fn parse_device_line(line: &str) -> Option<Device> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let id = parts[0].to_string();
    let status = parts[1].to_string();

    let mut model = String::new();
    let mut android_version = String::new();

    for chunk in &parts[2..] {
        if let Some(val) = chunk.strip_prefix("model:") {
            model = val.replace('_', " ");
        }
        if let Some(val) = chunk.strip_prefix("release:") {
            android_version = val.to_string();
        }
    }

    Some(Device {
        id,
        model,
        android_version,
        status: if status == "device" {
            "device".to_string()
        } else if status == "offline" {
            "offline".to_string()
        } else if status == "unauthorized" {
            "unauthorized".to_string()
        } else {
            status
        },
    })
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct LogLine {
    pub timestamp: String,
    pub pid: u32,
    pub tid: u32,
    pub level: String,
    pub tag: String,
    pub message: String,
}

#[allow(dead_code)]
pub fn parse_logcat_line(line: &str) -> Option<LogLine> {
    let line = line.trim();
    if line.is_empty() || !line.contains(": ") {
        return None;
    }

    let parts: Vec<&str> = line.splitn(6, ' ').collect();
    if parts.len() < 6 {
        return None;
    }

    let timestamp = parts[0].to_string();
    let pid: u32 = parts[1].parse().unwrap_or(0);
    let tid: u32 = parts[2]
        .trim_end_matches(')')
        .trim_start_matches('(')
        .parse()
        .unwrap_or(0);
    let level = parts[3].to_string();
    let tag_and_msg = parts[4..].join(" ");

    let colon_pos = tag_and_msg.find(": ")?;
    let tag = tag_and_msg[..colon_pos].to_string();
    let message = tag_and_msg[colon_pos + 2..].to_string();

    Some(LogLine {
        timestamp,
        pid,
        tid,
        level,
        tag,
        message,
    })
}

pub fn parse_ip_output(output: &str) -> String {
    for line in output.lines() {
        let line = line.trim();
        if line.contains("inet ") && !line.contains("127.0.0.1") {
            if let Some(start) = line.find("inet ") {
                let rest = &line[start + 5..];
                if let Some(end) = rest.find('/') {
                    return rest[..end].to_string();
                }
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::{parse_devices_output, parse_ip_output};

    #[test]
    fn parses_devices_and_deduplicates_ids() {
        let output = "List of devices attached\nABC123 device product:x model:Pixel_8 device:x\nABC123 device model:Duplicate\nOFFLINE offline\n";
        let devices = parse_devices_output(output);

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, "ABC123");
        assert_eq!(devices[0].model, "Pixel 8");
        assert_eq!(devices[1].status, "offline");
    }

    #[test]
    fn extracts_non_loopback_ip() {
        let output = "1: lo inet 127.0.0.1/8\n2: wlan0 inet 192.168.1.25/24 brd 192.168.1.255\n";
        assert_eq!(parse_ip_output(output), "192.168.1.25");
    }
}
