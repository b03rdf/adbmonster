mod adb;
mod commands;
mod error;
mod network_proxy;
mod network_scenario;
mod tasks;
mod types;

use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(tasks::TaskManager::default())
        .manage(commands::logcat::LogcatState::default())
        .manage(commands::media::RecordState {
            operation: tokio::sync::Mutex::new(()),
            session: Mutex::new(None),
        })
        .manage(commands::scrcpy::ScrcpyState::default())
        .manage(commands::diagnostics::DiagnosticState::default())
        .manage(commands::network::WeakNetworkState::default())
        .manage(commands::network_profiles::NetworkProfileStore::default())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .setup(|app| {
            let window = app.get_webview_window("main").ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "main window was not created")
            })?;
            if let Some(icon) = app.default_window_icon() {
                window.set_icon(icon.clone())?;
            }
            commands::tray::setup_tray(app)?;
            tasks::initialize(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tasks::list_tasks,
            tasks::cancel_task,
            tasks::clear_task_history,
            commands::devices::get_devices,
            commands::devices::connect_device,
            commands::devices::disconnect_device,
            commands::devices::get_device_ip,
            commands::devices::tcpip_connect,
            commands::devices::pair_device,
            commands::devices::pair_then_connect,
            commands::devices::auto_connect_local_emulator,
            commands::diagnostics::get_device_metrics,
            commands::diagnostics::create_diagnostic_package,
            commands::diagnostics::get_diagnostic_status,
            commands::logcat::start_logcat,
            commands::logcat::stop_logcat,
            commands::logcat::is_logcat_running,
            commands::logcat::export_logcat,
            commands::apps::install_apk,
            commands::apps::uninstall_apk,
            commands::apps::clear_app,
            commands::apps::get_package_info,
            commands::apps::pull_apk,
            commands::apps::list_packages,
            commands::media::take_screenshot,
            commands::media::pull_file,
            commands::media::pull_clog,
            commands::media::start_record,
            commands::media::stop_record,
            commands::media::release_recording,
            commands::media::is_recording,
            commands::media::get_recording_status,
            commands::scrcpy::start_scrcpy,
            commands::scrcpy::stop_scrcpy,
            commands::scrcpy::is_scrcpy_running,
            commands::files::list_remote_files,
            commands::files::push_file_to_remote,
            commands::files::delete_remote_file,
            commands::files::create_remote_directory,
            commands::network::detect_weak_network_capabilities,
            commands::network::install_weak_network_helper,
            commands::network::authorize_weak_network_helper,
            commands::network::apply_weak_network,
            commands::network::run_weak_network_scenario,
            commands::network_profiles::list_weak_network_scenarios,
            commands::network_profiles::save_weak_network_scenario,
            commands::network_profiles::delete_weak_network_scenario,
            commands::network_profiles::get_weak_network_observation,
            commands::network_profiles::export_weak_network_report,
            commands::network::stop_weak_network,
            commands::network::get_weak_network_status,
            commands::tray::update_tray_menu,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if !app
                    .state::<tasks::TaskManager>()
                    .exit_ready
                    .load(std::sync::atomic::Ordering::Acquire)
                {
                    api.prevent_exit();
                    tasks::shutdown(app);
                }
            }
        });
}
