mod adb;
mod commands;
mod error;
mod types;

use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::logcat::LogcatState {
            process: Mutex::new(None),
        })
        .manage(commands::media::RecordState {
            process: Mutex::new(None),
            remote_path: Mutex::new(None),
        })
        .manage(commands::scrcpy::ScrcpyState {
            process: Mutex::new(None),
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            if let Some(icon) = app.default_window_icon() {
                window.set_icon(icon.clone()).unwrap();
            }
            commands::tray::setup_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::devices::get_devices,
            commands::devices::connect_device,
            commands::devices::disconnect_device,
            commands::devices::get_device_ip,
            commands::devices::tcpip_connect,
            commands::devices::pair_device,
            commands::devices::pair_then_connect,
            commands::logcat::start_logcat,
            commands::logcat::stop_logcat,
            commands::logcat::is_logcat_running,
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
            commands::media::is_recording,
            commands::scrcpy::start_scrcpy,
            commands::scrcpy::stop_scrcpy,
            commands::scrcpy::is_scrcpy_running,
            commands::files::list_remote_files,
            commands::files::push_file_to_remote,
            commands::files::delete_remote_file,
            commands::files::create_remote_directory,
            commands::tray::update_tray_menu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
