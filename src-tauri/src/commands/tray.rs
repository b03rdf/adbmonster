use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

pub fn setup_tray<R: Runtime>(app: &impl Manager<R>) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItemBuilder::with_id("show", "显示窗口")
        .accelerator("CmdOrCtrl+Shift+S")
        .build(app)?;
    let info = MenuItemBuilder::with_id("info", "未连接设备")
        .enabled(false)
        .build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&info)
        .separator()
        .item(&quit)
        .build()?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../../icons/32x32.png"))?;

    TrayIconBuilder::with_id("main_tray")
        .icon(icon)
        .menu(&menu)
        .tooltip("adb缝合怪")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

#[tauri::command]
pub async fn update_tray_menu(app: AppHandle, device_summary: String) -> Result<(), String> {
    let tray = app.tray_by_id("main_tray").ok_or("系统托盘未找到")?;
    tray.set_tooltip(Some(&device_summary))
        .map_err(|e| e.to_string())?;
    Ok(())
}
