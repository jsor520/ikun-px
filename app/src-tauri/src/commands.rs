//! Tauri IPC surface for the independent Rust runtime.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::marslink::fake_dns::FakeDns;
use crate::marslink::progress::{self, Direction, Reporter};
use crate::marslink::quality::NodeMeasurement;
use crate::marslink::rules::{RouteDecision, RuleEngine};
use crate::marslink::runtime::{RuntimeStatus, DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT};
use crate::marslink::state::AppState;
use crate::marslink::{self, api, config, diagnostics, integrations, system_proxy};

const DEVELOPMENT_HTTP_PORT: u16 = 21888;
const DEVELOPMENT_SOCKS_PORT: u16 = 21889;

#[derive(Debug, Clone, Serialize)]
pub struct EngineStatus {
    pub http_proxy_open: bool,
    pub socks5_open: bool,
    pub socks5_ok: bool,
    pub process_running: bool,
    pub tun_running: bool,
    pub node_uuid: String,
    pub node_name: String,
    pub http_address: String,
    pub socks5_address: String,
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticReport {
    pub checks: Vec<DiagnosticCheck>,
    /// Handshake against the address the tunnel is using, absent when no node
    /// is selected yet.
    pub node: Option<diagnostics::Probe>,
    /// Page load through the local proxy, absent while disconnected.
    pub web: Option<diagnostics::Probe>,
}

impl EngineStatus {
    fn from_runtime(runtime: RuntimeStatus, tun_running: bool) -> Self {
        Self {
            http_proxy_open: runtime.running,
            socks5_open: runtime.running,
            socks5_ok: runtime.running,
            process_running: runtime.running,
            tun_running,
            node_uuid: runtime.node_uuid,
            node_name: runtime.node_name,
            http_address: runtime.http_address,
            socks5_address: runtime.socks5_address,
            upload_bytes: runtime.upload_bytes,
            download_bytes: runtime.download_bytes,
        }
    }
}

#[tauri::command]
pub fn get_config() -> Result<config::MarsConfig, String> {
    config::read_config()
}

#[tauri::command]
pub fn set_ui_preferences(
    show_node_icons: bool,
    sidebar_collapsed: bool,
) -> Result<config::MarsConfig, String> {
    config::set_ui_preferences(show_node_icons, sidebar_collapsed)?;
    config::read_config()
}

#[tauri::command]
pub async fn get_user_info() -> Result<api::UserInfo, String> {
    let ctx = marslink::build_context()?;
    api::user_info(&ctx).await
}

#[tauri::command]
pub async fn get_nodes(state: State<'_, AppState>) -> Result<Vec<api::Node>, String> {
    let ctx = marslink::build_context()?;
    let nodes = api::node_list(&ctx).await?;
    state.quality.track(nodes.clone()).await;
    Ok(nodes)
}

/// Reads what the background prober has collected so far; the UI polls this, so
/// it must stay local and cheap.
#[tauri::command]
pub async fn measure_nodes(state: State<'_, AppState>) -> Result<Vec<NodeMeasurement>, String> {
    Ok(state.quality.snapshot())
}

#[tauri::command]
pub async fn login(
    state: State<'_, AppState>,
    email: String,
    password: String,
    remember_password: bool,
) -> Result<(), String> {
    state.shutdown().await?;
    let context = marslink::build_login_context()?;
    let result = api::login(&context, &email, &password).await?;
    config::save_login(
        &result.email,
        &result.token,
        remember_password.then_some(password.as_str()),
    )?;
    Ok(())
}

#[tauri::command]
pub fn get_saved_credentials() -> Result<config::SavedCredentials, String> {
    config::saved_credentials()
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>, forget: bool) -> Result<(), String> {
    state.shutdown().await?;
    state.quality.stop().await;
    config::clear_login(forget)
}

#[tauri::command]
pub async fn get_init_config() -> Result<api::InitConfig, String> {
    let context = marslink::build_context()?;
    api::init_config(&context).await
}

#[tauri::command]
pub fn get_user_rules() -> Result<crate::marslink::rules::UserRules, String> {
    crate::marslink::rules::read_user_rules()
}

#[tauri::command]
pub async fn save_user_rules(
    state: State<'_, AppState>,
    rules: crate::marslink::rules::UserRules,
) -> Result<(), String> {
    crate::marslink::rules::write_user_rules(&rules)?;
    let mode = config::read_config()?.route_mode;
    state
        .runtime
        .replace_rules(RuleEngine::load_cached(mode))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn set_domain_route(
    state: State<'_, AppState>,
    host: String,
    route: RouteDecision,
) -> Result<crate::marslink::rules::UserRules, String> {
    let rules = crate::marslink::rules::set_domain_route(&host, route)?;
    let mode = config::read_config()?.route_mode;
    state
        .runtime
        .replace_rules(RuleEngine::load_cached(mode))
        .await?;
    Ok(rules)
}

#[tauri::command]
pub async fn clear_domain_route(
    state: State<'_, AppState>,
    host: String,
) -> Result<crate::marslink::rules::UserRules, String> {
    let rules = crate::marslink::rules::clear_domain_route(&host)?;
    let mode = config::read_config()?.route_mode;
    state
        .runtime
        .replace_rules(RuleEngine::load_cached(mode))
        .await?;
    Ok(rules)
}

#[tauri::command]
pub fn get_pinned_domains() -> Result<crate::marslink::rules::PinnedDomains, String> {
    crate::marslink::rules::pinned_domains()
}

#[tauri::command]
pub fn copy_proxy_environment(app: AppHandle) -> Result<(), String> {
    let (http_port, socks_port) = proxy_ports();
    app.clipboard()
        .write_text(integrations::environment_text(http_port, socks_port))
        .map_err(|e| format!("copy proxy environment: {e}"))
}

#[tauri::command]
pub fn copy_proxy_addresses(app: AppHandle) -> Result<(), String> {
    let (http_port, socks_port) = proxy_ports();
    app.clipboard()
        .write_text(format!(
            "HTTP=http://127.0.0.1:{http_port}\nSOCKS5=socks5://127.0.0.1:{socks_port}"
        ))
        .map_err(|e| format!("copy proxy addresses: {e}"))
}

#[tauri::command]
pub fn open_proxy_console() -> Result<(), String> {
    let (http_port, socks_port) = proxy_ports();
    integrations::launch_proxy_console(http_port, socks_port)
}

#[tauri::command]
pub async fn get_domain_events(
    state: State<'_, AppState>,
) -> Result<Vec<crate::marslink::proxy::DomainEvent>, String> {
    Ok(state.runtime.domain_events().await)
}

#[tauri::command]
pub async fn clear_domain_events(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.clear_domain_events().await;
    Ok(())
}

#[tauri::command]
pub async fn set_integrations(
    state: State<'_, AppState>,
    git_proxy: bool,
    ssh_proxy: bool,
    auto_connect: bool,
    auto_start: bool,
    hotkeys_enabled: bool,
) -> Result<(), String> {
    let (http_port, socks_port) = proxy_ports();
    let running = state.runtime.status().await.running;
    if !git_proxy || running {
        integrations::set_git_proxy(git_proxy, http_port)?;
    }
    if !ssh_proxy || running {
        integrations::set_ssh_proxy(ssh_proxy, socks_port)?;
    }
    if !cfg!(debug_assertions) {
        integrations::set_auto_start(auto_start)?;
    }
    config::set_integrations(
        git_proxy,
        ssh_proxy,
        auto_connect,
        auto_start,
        hotkeys_enabled,
    )
}

#[tauri::command]
pub async fn repair_network(state: State<'_, AppState>) -> Result<String, String> {
    state.shutdown().await?;
    let stale_proxy_disabled = system_proxy::repair_stale_loopback_proxy()?;
    Ok(if stale_proxy_disabled {
        String::from("已恢复 IKUN PX 网络状态，并禁用无监听器的失效本地代理")
    } else {
        String::from("已恢复 IKUN PX 所有代理、TUN、DNS 和路由状态")
    })
}

#[tauri::command]
pub async fn network_diagnostics(state: State<'_, AppState>) -> Result<DiagnosticReport, String> {
    let runtime = state.runtime.status().await;
    let tun_running = state.tun.is_running().await;
    let mut checks = vec![
        DiagnosticCheck {
            name: String::from("Rust 代理内核"),
            ok: runtime.running,
            detail: if runtime.running {
                format!("{} / {}", runtime.http_address, runtime.socks5_address)
            } else {
                String::from("当前未连接")
            },
        },
        DiagnosticCheck {
            name: String::from("TUN 进程"),
            ok: tun_running,
            detail: if tun_running {
                String::from("tun2socks v2.6 正在运行")
            } else {
                String::from("未启用")
            },
        },
    ];

    if tun_running {
        let adapter = crate::marslink::routing::adapter_index().await;
        checks.push(DiagnosticCheck {
            name: String::from("Wintun 网卡"),
            ok: adapter.is_ok(),
            detail: adapter
                .map(|index| format!("ikun-px ifIndex={index}"))
                .unwrap_or_else(|error| error),
        });
    }

    if let Ok(proxy) = system_proxy::status() {
        checks.push(DiagnosticCheck {
            name: String::from("Windows 系统代理"),
            ok: !proxy.enabled || proxy.listener_alive,
            detail: if proxy.enabled {
                format!(
                    "{} ({})",
                    proxy.server,
                    if proxy.listener_alive {
                        "监听正常"
                    } else {
                        "端口无监听"
                    }
                )
            } else {
                String::from("未启用")
            },
        });
    }

    let context = marslink::build_context();
    let started = std::time::Instant::now();
    let api_check = match &context {
        Ok(context) => api::user_info(context).await.map(|_| started.elapsed()),
        Err(error) => Err(error.clone()),
    };
    checks.push(DiagnosticCheck {
        name: String::from("控制接口"),
        ok: api_check.is_ok(),
        detail: api_check
            .map(|elapsed| format!("{} ms", elapsed.as_millis()))
            .unwrap_or_else(|error| error),
    });

    // Disconnected the button still has something to answer, so it falls back
    // to the node that connecting would use.
    let target = match state.runtime.current_node().await {
        Some(node) => Some(node),
        None => match context {
            Ok(context) => selected_node(&context).await,
            Err(_) => None,
        },
    };
    let node = match target {
        Some(node) => Some(diagnostics::node_probe(&node).await),
        None => None,
    };
    let web = if runtime.running {
        Some(diagnostics::web_probe(proxy_ports().0).await)
    } else {
        None
    };

    Ok(DiagnosticReport { checks, node, web })
}

async fn selected_node(context: &api::ApiContext) -> Option<api::Node> {
    let uuid = config::read_config().ok()?.last_select_node;
    api::node_list(context)
        .await
        .ok()?
        .into_iter()
        .find(|node| node.uuid == uuid)
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<EngineStatus, String> {
    let runtime = state.runtime.status().await;
    let tun_running = state.tun.is_running().await;
    Ok(EngineStatus::from_runtime(runtime, tun_running))
}

#[tauri::command]
pub async fn switch_node(
    app: AppHandle,
    state: State<'_, AppState>,
    uuid: String,
) -> Result<(), String> {
    if !is_uuid(&uuid) {
        return Err(String::from("invalid node UUID"));
    }
    let previous = config::read_config()?.last_select_node;
    let was_running = state.runtime.status().await.running;
    config::set_selected_node(&uuid)?;

    if was_running {
        let progress = Reporter::new(app, Direction::Connect, progress::RUNTIME_STAGES + 1);
        let result = start_proxy_runtime(&state, &progress).await;
        if let Err(error) = result {
            let _ = config::set_selected_node(&previous);
            let _ = start_proxy_runtime(&state, &Reporter::silent()).await;
            return Err(error);
        }
        progress.done("ready", "代理服务已就绪");
    }
    Ok(())
}

#[tauri::command]
pub async fn vpn_start(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.wait_for_recovery().await;
    let config = config::read_config()?;
    let progress = Reporter::new(app, Direction::Connect, connect_stages(&config));
    let runtime = start_proxy_runtime(&state, &progress).await?;
    let (http_port, socks_port) = proxy_ports();

    if network_changes_allowed() {
        let capture_result = match config.capture_mode {
            config::CaptureMode::Local => Ok(()),
            config::CaptureMode::System => {
                progress.stage("system-proxy", "接管系统代理");
                system_proxy::enable(http_port)
            }
            config::CaptureMode::Tun => {
                progress.stage("tun", "启动 TUN 网卡接管");
                state.tun.start(socks_port).await
            }
        };
        if let Err(error) = capture_result {
            return Err(rollback_failed_start(&state, error).await);
        }
    }
    if config.git_proxy {
        progress.stage("git", "配置 Git 代理");
        if let Err(error) = integrations::set_git_proxy(true, http_port) {
            return Err(rollback_failed_start(&state, error).await);
        }
    }
    if config.ssh_proxy {
        progress.stage("ssh", "配置 SSH 代理");
        if let Err(error) = integrations::set_ssh_proxy(true, socks_port) {
            return Err(rollback_failed_start(&state, error).await);
        }
    }
    if !runtime.running {
        return Err(String::from("independent proxy runtime failed to start"));
    }
    progress.done("ready", route_ready_message(config.route_mode));
    Ok(())
}

#[tauri::command]
pub async fn vpn_stop(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let progress = Reporter::new(app, Direction::Disconnect, progress::SHUTDOWN_STAGES);
    state.shutdown_reporting(&progress).await
}

/// The optional integrations make the stage count configuration dependent, so
/// the UI is told the real total up front rather than guessing.
fn connect_stages(config: &config::MarsConfig) -> u8 {
    let mut total = progress::RUNTIME_STAGES + 1;
    if network_changes_allowed() && config.capture_mode != config::CaptureMode::Local {
        total += 1;
    }
    if config.git_proxy {
        total += 1;
    }
    if config.ssh_proxy {
        total += 1;
    }
    total
}

#[tauri::command]
pub async fn set_network_modes(
    state: State<'_, AppState>,
    capture_mode: config::CaptureMode,
    route_mode: config::RouteMode,
) -> Result<config::MarsConfig, String> {
    state.wait_for_recovery().await;
    let previous = config::read_config()?;
    config::set_network_modes(capture_mode, route_mode)?;

    if state.runtime.status().await.running {
        let result: Result<(), String> = async {
            state.runtime.set_route_mode(route_mode).await?;
            if capture_mode != previous.capture_mode {
                apply_capture_mode(&state, capture_mode).await?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            let mut rollback_errors = Vec::new();
            if let Err(rollback) =
                config::set_network_modes(previous.capture_mode, previous.route_mode)
            {
                rollback_errors.push(rollback);
            }
            if let Err(rollback) = state.runtime.set_route_mode(previous.route_mode).await {
                rollback_errors.push(rollback);
            }
            if let Err(rollback) = apply_capture_mode(&state, previous.capture_mode).await {
                rollback_errors.push(rollback);
            }
            if rollback_errors.is_empty() {
                return Err(format!("{error}；已恢复原接管方式"));
            }
            return Err(rollback_failed_start(
                &state,
                format!(
                    "{error}；恢复原接管方式失败：{}",
                    rollback_errors.join("；")
                ),
            )
            .await);
        }
    }
    config::read_config()
}

async fn apply_capture_mode(
    state: &AppState,
    capture_mode: config::CaptureMode,
) -> Result<(), String> {
    if !network_changes_allowed() {
        return Ok(());
    }
    let (http_port, socks_port) = proxy_ports();
    match capture_mode {
        config::CaptureMode::Local => {
            let proxy_result = system_proxy::restore();
            let tun_result = state.tun.stop().await;
            proxy_result?;
            tun_result
        }
        config::CaptureMode::System => {
            system_proxy::enable(http_port)?;
            state.tun.stop().await
        }
        config::CaptureMode::Tun => {
            state.tun.start(socks_port).await?;
            system_proxy::restore()
        }
    }
}

async fn rollback_failed_start(state: &AppState, cause: String) -> String {
    let mut cleanup_errors = Vec::new();
    for _ in 0..2 {
        if let Err(error) = state.shutdown().await {
            cleanup_errors.push(error);
        }
        let clean = !state.tun.is_running().await
            && !state.runtime.status().await.running
            && !crate::marslink::routing::has_stale_state()
            && !system_proxy::has_snapshot();
        if clean {
            return format!("{cause}；已自动恢复原网络");
        }
    }
    let detail = if cleanup_errors.is_empty() {
        String::from("仍检测到本程序占用的网络资源")
    } else {
        cleanup_errors.join("；")
    };
    format!("{cause}；自动恢复未完成：{detail}。请使用“代理管理 → 修复网络”")
}

/// Reports [`progress::RUNTIME_STAGES`] stages; keep the two in step.
async fn start_proxy_runtime(
    state: &AppState,
    progress: &Reporter,
) -> Result<RuntimeStatus, String> {
    progress.stage("config", "读取本地配置");
    let config = config::read_config()?;
    progress.stage("credentials", "校验登录凭据");
    let context = marslink::build_context()?;
    progress.stage("nodes", "获取节点列表");
    let nodes = api::node_list(&context).await?;
    progress.stage("node", "锁定目标节点");
    let node = nodes
        .into_iter()
        .find(|node| node.uuid == config.last_select_node)
        .ok_or_else(|| String::from("selected node is unavailable"))?;
    progress.stage("parameters", "拉取线路参数");
    let init = api::init_config(&context).await.unwrap_or(api::InitConfig {
        sys_proxy_file: String::new(),
        sys_local_file: String::new(),
        country_file: String::new(),
        public_msg_title: String::new(),
        public_msg_content: String::new(),
    });
    progress.stage("rules", "加载分流规则");
    let rules = RuleEngine::load_or_refresh(
        &init.sys_proxy_file,
        &init.sys_local_file,
        RouteDecision::Proxy,
    )
    .await?
    .with_mode(config.route_mode);

    progress.stage("core", "启动本地代理内核");
    state
        .runtime
        .start_with_rules(
            node,
            context.token,
            proxy_ports().0,
            proxy_ports().1,
            Arc::new(rules),
            Arc::new(FakeDns::default()),
        )
        .await
}

fn proxy_ports() -> (u16, u16) {
    if cfg!(debug_assertions) {
        (DEVELOPMENT_HTTP_PORT, DEVELOPMENT_SOCKS_PORT)
    } else {
        (DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT)
    }
}

fn network_changes_allowed() -> bool {
    !cfg!(debug_assertions)
}

fn route_ready_message(mode: config::RouteMode) -> &'static str {
    match mode {
        config::RouteMode::Smart => "智能分流已启用",
        config::RouteMode::Global => "全局代理已启用",
        config::RouteMode::Direct => "全部直连已启用",
    }
}

fn is_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && [8, 4, 4, 4, 12].iter().zip(groups).all(|(length, group)| {
            group.len() == *length && group.bytes().all(|b| b.is_ascii_hexdigit())
        })
}
