mod commands;
pub mod marslink;
mod tray;

use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(marslink::state::AppState::new())
        .setup(|app| {
            tray::setup(app)?;

            // Register separately and ignore conflicts. This lets the new client
            // run alongside an old client that still owns the same hotkeys.
            let _ = app
                .global_shortcut()
                .on_shortcut("ctrl+alt+s", |app, _, event| {
                    if event.state == ShortcutState::Pressed
                        && marslink::config::read_config()
                            .map(|config| config.hotkeys_enabled)
                            .unwrap_or(true)
                    {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                });
            let _ = app
                .global_shortcut()
                .on_shortcut("ctrl+alt+t", |_, _, event| {
                    if event.state == ShortcutState::Pressed
                        && marslink::config::read_config()
                            .map(|config| config.hotkeys_enabled)
                            .unwrap_or(true)
                    {
                        let _ = marslink::integrations::launch_proxy_console(
                            marslink::runtime::DEFAULT_HTTP_PORT,
                            marslink::runtime::DEFAULT_SOCKS_PORT,
                        );
                    }
                });

            let recovery_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                recovery_handle
                    .state::<marslink::state::AppState>()
                    .recover_owned_network_state()
                    .await;
            });

            if let Some(window) = app.get_webview_window("main") {
                let window_to_hide = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_to_hide.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_ui_preferences,
            commands::get_user_info,
            commands::get_nodes,
            commands::measure_nodes,
            commands::login,
            commands::logout,
            commands::get_saved_credentials,
            commands::get_init_config,
            commands::get_user_rules,
            commands::save_user_rules,
            commands::set_domain_route,
            commands::clear_domain_route,
            commands::get_pinned_domains,
            commands::copy_proxy_environment,
            commands::copy_proxy_addresses,
            commands::open_proxy_console,
            commands::get_domain_events,
            commands::clear_domain_events,
            commands::set_integrations,
            commands::repair_network,
            commands::network_diagnostics,
            commands::get_status,
            commands::switch_node,
            commands::vpn_start,
            commands::vpn_stop,
            commands::set_network_modes,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
