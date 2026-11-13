use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::marslink::runtime::{DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT};
use crate::marslink::state::AppState;
use crate::marslink::{config, integrations};

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let config = config::read_config().unwrap_or_else(|_| fallback_config());
    let copy_env = MenuItem::with_id(app, "copy_env", "复制代理环境变量", true, None::<&str>)?;
    let console = MenuItem::with_id(
        app,
        "proxy_console",
        "控制台代理 (Ctrl+Alt+T)",
        true,
        None::<&str>,
    )?;
    let addresses = MenuItem::with_id(
        app,
        "copy_addresses",
        "复制本地代理地址",
        true,
        None::<&str>,
    )?;
    let proxy_manager = MenuItem::with_id(app, "proxy_manager", "代理管理", true, None::<&str>)?;
    let network_modes =
        MenuItem::with_id(app, "network_modes", "网络与分流模式…", true, None::<&str>)?;
    let git_proxy = CheckMenuItem::with_id(
        app,
        "git_proxy",
        "自动设置 Git 代理",
        true,
        config.git_proxy,
        None::<&str>,
    )?;
    let ssh_proxy = CheckMenuItem::with_id(
        app,
        "ssh_proxy",
        "自动设置 SSH 代理",
        true,
        config.ssh_proxy,
        None::<&str>,
    )?;
    let proxy_rules =
        MenuItem::with_id(app, "proxy_rules", "配置走代理的域名", true, None::<&str>)?;
    let direct_rules =
        MenuItem::with_id(app, "direct_rules", "配置走本地的域名", true, None::<&str>)?;
    let switch_user = MenuItem::with_id(app, "switch_user", "切换用户", true, None::<&str>)?;
    let clear_login = MenuItem::with_id(app, "clear_login", "清空登录", true, None::<&str>)?;
    let announcement = MenuItem::with_id(app, "announcement", "公告消息", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "显示主窗口 (Ctrl+Alt+S)", true, None::<&str>)?;
    let repair = MenuItem::with_id(app, "repair", "修复本地网络", true, None::<&str>)?;
    let hotkeys = CheckMenuItem::with_id(
        app,
        "hotkeys",
        "启用应用热键",
        true,
        config.hotkeys_enabled,
        None::<&str>,
    )?;
    let auto_connect = CheckMenuItem::with_id(
        app,
        "auto_connect",
        "运行自动加速",
        true,
        config.auto_connect,
        None::<&str>,
    )?;
    let auto_start = CheckMenuItem::with_id(
        app,
        "auto_start",
        "开机自启",
        true,
        config.auto_start,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &copy_env,
            &console,
            &addresses,
            &PredefinedMenuItem::separator(app)?,
            &proxy_manager,
            &network_modes,
            &git_proxy,
            &ssh_proxy,
            &proxy_rules,
            &direct_rules,
            &PredefinedMenuItem::separator(app)?,
            &switch_user,
            &clear_login,
            &announcement,
            &show,
            &PredefinedMenuItem::separator(app)?,
            &repair,
            &hotkeys,
            &auto_connect,
            &auto_start,
            &quit,
        ],
    )?;

    let git_item = git_proxy.clone();
    let ssh_item = ssh_proxy.clone();
    let hotkeys_item = hotkeys.clone();
    let auto_connect_item = auto_connect.clone();
    let auto_start_item = auto_start.clone();

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("IKUN PX");
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                tauri::tray::TrayIconEvent::DoubleClick {
                    button: tauri::tray::MouseButton::Left,
                    ..
                }
            ) {
                show_main(tray.app_handle());
            }
        })
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "copy_env" => {
                let _ = app.clipboard().write_text(integrations::environment_text(
                    DEFAULT_HTTP_PORT,
                    DEFAULT_SOCKS_PORT,
                ));
            }
            "copy_addresses" => {
                let _ = app.clipboard().write_text(format!(
                    "HTTP=http://127.0.0.1:{DEFAULT_HTTP_PORT}\nSOCKS5=socks5://127.0.0.1:{DEFAULT_SOCKS_PORT}"
                ));
            }
            "proxy_console" => {
                let _ =
                    integrations::launch_proxy_console(DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT);
            }
            "proxy_manager" => show_and_emit(app, "navigate-section", "proxy-manager"),
            "network_modes" => show_and_emit(app, "navigate-section", "overview"),
            "proxy_rules" | "direct_rules" => show_and_emit(app, "navigate-section", "rules"),
            "announcement" => show_and_emit(app, "show-announcement", ""),
            "show" => show_main(app),
            "switch_user" => show_and_emit(app, "request-logout", ""),
            "clear_login" => show_and_emit(app, "request-logout", "forget"),
            "repair" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let result = handle.state::<AppState>().shutdown().await;
                    let message = result
                        .map(|_| "本地网络已恢复".to_string())
                        .unwrap_or_else(|error| format!("网络恢复失败：{error}"));
                    let _ = handle.emit("network-repaired", message);
                });
            }
            "git_proxy" => apply_integration_toggle(
                app,
                git_item.is_checked().unwrap_or(false),
                IntegrationKind::Git,
            ),
            "ssh_proxy" => apply_integration_toggle(
                app,
                ssh_item.is_checked().unwrap_or(false),
                IntegrationKind::Ssh,
            ),
            "hotkeys" => update_preference(
                hotkeys_item.is_checked().unwrap_or(true),
                PreferenceKind::Hotkeys,
            ),
            "auto_connect" => update_preference(
                auto_connect_item.is_checked().unwrap_or(false),
                PreferenceKind::AutoConnect,
            ),
            "auto_start" => {
                let checked = auto_start_item.is_checked().unwrap_or(false);
                if !cfg!(debug_assertions) {
                    let _ = integrations::set_auto_start(checked);
                }
                update_preference(checked, PreferenceKind::AutoStart);
            }
            "quit" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = handle.state::<AppState>().shutdown().await;
                    handle.exit(0);
                });
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn show_and_emit(app: &tauri::AppHandle, event: &str, payload: &str) {
    show_main(app);
    let _ = app.emit(event, payload);
}

#[derive(Clone, Copy)]
enum IntegrationKind {
    Git,
    Ssh,
}

fn apply_integration_toggle(app: &tauri::AppHandle, enabled: bool, kind: IntegrationKind) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let running = handle.state::<AppState>().runtime.status().await.running;
        let result = if !enabled || running {
            match kind {
                IntegrationKind::Git => integrations::set_git_proxy(enabled, DEFAULT_HTTP_PORT),
                IntegrationKind::Ssh => integrations::set_ssh_proxy(enabled, DEFAULT_SOCKS_PORT),
            }
        } else {
            Ok(())
        };
        if let Err(error) = result {
            let _ = handle.emit("integration-error", error);
            return;
        }
        if let Ok(current) = config::read_config() {
            let (git, ssh) = match kind {
                IntegrationKind::Git => (enabled, current.ssh_proxy),
                IntegrationKind::Ssh => (current.git_proxy, enabled),
            };
            let _ = config::set_integrations(
                git,
                ssh,
                current.auto_connect,
                current.auto_start,
                current.hotkeys_enabled,
            );
        }
        let _ = handle.emit("config-changed", ());
    });
}

enum PreferenceKind {
    Hotkeys,
    AutoConnect,
    AutoStart,
}

fn update_preference(enabled: bool, kind: PreferenceKind) {
    if let Ok(current) = config::read_config() {
        let (auto_connect, auto_start, hotkeys) = match kind {
            PreferenceKind::Hotkeys => (current.auto_connect, current.auto_start, enabled),
            PreferenceKind::AutoConnect => (enabled, current.auto_start, current.hotkeys_enabled),
            PreferenceKind::AutoStart => (current.auto_connect, enabled, current.hotkeys_enabled),
        };
        let _ = config::set_integrations(
            current.git_proxy,
            current.ssh_proxy,
            auto_connect,
            auto_start,
            hotkeys,
        );
    }
}

fn fallback_config() -> config::MarsConfig {
    config::MarsConfig {
        token: String::new(),
        email: String::new(),
        last_select_node: String::new(),
        capture_mode: config::CaptureMode::Local,
        route_mode: config::RouteMode::Smart,
        api: String::new(),
        web: String::new(),
        device_id: String::new(),
        password_saved: false,
        logged_in: false,
        git_proxy: false,
        ssh_proxy: false,
        auto_connect: false,
        auto_start: false,
        hotkeys_enabled: true,
        show_node_icons: false,
        sidebar_collapsed: false,
    }
}
