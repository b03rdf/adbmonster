use super::network::{active_report, WeakNetworkState};
use crate::network_scenario::{NetworkReport, NetworkScenario};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct NetworkProfileStore {
    operation: tokio::sync::Mutex<()>,
}

fn store_path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|directory| directory.join(name))
        .map_err(|error| error.to_string())
}

async fn read_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    limit: u64,
) -> Result<Option<T>, String> {
    use tokio::io::AsyncReadExt;
    let metadata = match tokio::fs::metadata(path).await {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.len() > limit {
        return Err("配置/报告文件超过大小限制，未加载".into());
    }
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("配置/报告文件超过大小限制，未加载".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("JSON 文件无效（未覆盖原文件）：{error}"))
}

pub(super) async fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| error.to_string())?;
    let name = path
        .file_name()
        .ok_or_else(|| "目标文件名不能为空".to_string())?
        .to_string_lossy();
    let temporary = parent.join(format!(
        ".{name}.{}.partial",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .await
        .map_err(|error| error.to_string())?;
    let result = async {
        file.write_all(&bytes)
            .await
            .map_err(|error| error.to_string())?;
        file.sync_all().await.map_err(|error| error.to_string())?;
        drop(file);
        tokio::fs::rename(&temporary, path)
            .await
            .map_err(|error| error.to_string())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

async fn load_scenarios(app: &AppHandle) -> Result<Vec<NetworkScenario>, String> {
    let scenarios: Vec<NetworkScenario> = read_json(
        &store_path(app, "weak-network-scenarios.json")?,
        1024 * 1024,
    )
    .await?
    .unwrap_or_default();
    if scenarios.len() > 50 {
        return Err("保存的场景超过 50 个".into());
    }
    for scenario in &scenarios {
        scenario.validate()?;
    }
    Ok(scenarios)
}

#[tauri::command]
pub async fn list_weak_network_scenarios(app: AppHandle) -> Result<Vec<NetworkScenario>, String> {
    let state = app.state::<NetworkProfileStore>();
    let _operation = state.operation.lock().await;
    load_scenarios(&app).await
}

#[tauri::command]
pub async fn save_weak_network_scenario(
    mut scenario: NetworkScenario,
    app: AppHandle,
) -> Result<Vec<NetworkScenario>, String> {
    scenario.name = scenario.name.trim().to_string();
    scenario.validate()?;
    let state = app.state::<NetworkProfileStore>();
    let _operation = state.operation.lock().await;
    let mut saved = load_scenarios(&app).await?;
    if let Some(existing) = saved
        .iter_mut()
        .find(|existing| existing.name == scenario.name)
    {
        *existing = scenario;
    } else {
        if saved.len() == 50 {
            return Err("最多保存 50 个场景，请删除旧场景后再保存".into());
        }
        saved.push(scenario);
    }
    write_json(&store_path(&app, "weak-network-scenarios.json")?, &saved).await?;
    Ok(saved)
}

#[tauri::command]
pub async fn delete_weak_network_scenario(
    name: String,
    app: AppHandle,
) -> Result<Vec<NetworkScenario>, String> {
    let state = app.state::<NetworkProfileStore>();
    let _operation = state.operation.lock().await;
    let mut saved = load_scenarios(&app).await?;
    saved.retain(|scenario| scenario.name != name);
    write_json(&store_path(&app, "weak-network-scenarios.json")?, &saved).await?;
    Ok(saved)
}

#[tauri::command]
pub async fn get_weak_network_observation(
    device_id: String,
    app: AppHandle,
) -> Result<Option<NetworkReport>, String> {
    if let Some(report) = active_report(&app).filter(|report| report.device_id == device_id) {
        return Ok(Some(report));
    }
    let state = app.state::<WeakNetworkState>();
    let cached = state
        .last_report
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    if let Some(report) = cached {
        return Ok((report.device_id == device_id).then_some(report));
    }
    let report: Option<NetworkReport> = read_json(
        &store_path(&app, "last-weak-network-report.json")?,
        8 * 1024 * 1024,
    )
    .await?;
    if let Some(report) = &report {
        if report.schema_version != 1 {
            return Err("不支持的弱网报告版本".into());
        }
        report.configured.validate()?;
    }
    // A run may finish while the disk read is in flight. Never replace its newer report.
    let mut cached = state
        .last_report
        .lock()
        .map_err(|error| error.to_string())?;
    if cached.is_none() {
        *cached = report;
    }
    Ok(cached
        .clone()
        .filter(|report| report.device_id == device_id))
}

pub(super) async fn persist_report(app: &AppHandle, report: &NetworkReport) -> Result<(), String> {
    write_json(&store_path(app, "last-weak-network-report.json")?, report).await
}

#[tauri::command]
pub async fn export_weak_network_report(
    run_id: String,
    output_path: String,
    app: AppHandle,
) -> Result<String, String> {
    if output_path.trim().is_empty() {
        return Err("报告输出路径不能为空".into());
    }
    let report = active_report(&app)
        .filter(|report| report.run_id == run_id)
        .or_else(|| {
            app.state::<WeakNetworkState>()
                .last_report
                .lock()
                .ok()?
                .clone()
                .filter(|report| report.run_id == run_id)
        })
        .ok_or_else(|| "当前报告已变化，请刷新后重新导出".to_string())?;
    write_json(Path::new(&output_path), &report).await?;
    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "adb-monster-network-{name}-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ))
    }

    #[tokio::test]
    async fn atomic_save_can_replace_an_existing_config_on_windows() {
        let path = test_path("replace");
        let mut scenario = NetworkScenario::single(&crate::types::WeakNetworkConfig {
            target_package: "com.example.game".into(),
            duration_seconds: 10,
            upload_kbps: 0,
            download_kbps: 0,
            latency_ms: 0,
            jitter_ms: 0,
            loss_percent: 0.0,
            duplicate_percent: 0.0,
            reorder_percent: 0.0,
        });
        scenario.validate().unwrap();
        write_json(&path, &vec![scenario.clone()]).await.unwrap();
        scenario.seed = u32::MAX;
        write_json(&path, &vec![scenario.clone()]).await.unwrap();
        let restored: Vec<NetworkScenario> = read_json(&path, 1024 * 1024).await.unwrap().unwrap();
        assert_eq!(restored, vec![scenario]);
        tokio::fs::remove_file(path).await.unwrap();
    }

    #[tokio::test]
    async fn missing_invalid_and_oversized_files_are_not_silently_overwritten() {
        let path = test_path("invalid");
        assert!(read_json::<NetworkReport>(&path, 1024)
            .await
            .unwrap()
            .is_none());
        write_json(&path, &"not a scenario collection")
            .await
            .unwrap();
        let before = tokio::fs::read(&path).await.unwrap();
        assert!(read_json::<Vec<NetworkScenario>>(&path, 1024)
            .await
            .is_err());
        assert!(read_json::<String>(&path, 2).await.is_err());
        assert_eq!(tokio::fs::read(&path).await.unwrap(), before);
        tokio::fs::remove_file(path).await.unwrap();
    }
}
