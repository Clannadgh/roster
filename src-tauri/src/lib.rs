mod account;
mod checkin_marks;
mod checkin_qoder;
mod checkin_trae;
mod checkin_workbuddy;
mod commands;
mod credits;
mod login;
mod login_qoder;
mod login_trae;
mod login_workbuddy;
mod process;
mod qoder_switch;
mod schedule;
mod scope;
mod secrets;
mod store;
mod switch_workbuddy;
mod trae_switch;
mod travel;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

#[derive(Serialize)]
struct AppInfo {
    name: &'static str,
    version: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "Roster",
        version: env!("CARGO_PKG_VERSION"),
    }
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开主界面", true, None::<&str>)?;
    let checkin = MenuItem::with_id(app, "checkin", "立即签到", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &checkin, &quit])?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Roster")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "checkin" => {
                tauri::async_runtime::spawn(async move {
                    let _ = schedule::run_all_scopes().await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if !argv.iter().any(|arg| arg == "--hidden") {
                show_main(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .setup(|app| {
            schedule::spawn();
            travel::spawn();
            setup_tray(app)?;

            if std::env::args().any(|arg| arg == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            if let Some(window) = app.get_webview_window("main") {
                let handle = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = handle.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            commands::list_accounts,
            commands::login_start,
            commands::login_poll,
            commands::import_trae_jwt,
            commands::delete_account,
            commands::switch_account,
            commands::save_trae_profile,
            commands::checkin_status,
            commands::checkin_account,
            commands::checkin_all,
            commands::checkin_all_scopes,
            commands::get_credits_summary,
            commands::travel_status,
            commands::travel_depart,
            commands::travel_claim,
            commands::get_travel_auto,
            commands::set_travel_auto,
            commands::get_schedule,
            commands::save_schedule,
            commands::get_autostart,
            commands::set_autostart
        ])
        .run(tauri::generate_context!())
        .expect("error while running Roster");
}
