//! Independent MarsLink compatibility layer: control API, wire transport,
//! local proxies, TUN lifecycle, configuration and encryption.

pub mod api;
pub mod config;
pub mod crypto;
pub mod diagnostics;
pub mod fake_dns;
pub mod integrations;
pub mod progress;
pub mod proxy;
pub mod quality;
pub mod routing;
pub mod rules;
pub mod runtime;
pub mod state;
pub mod system_proxy;
pub mod transport;
pub mod tun;

use api::ApiContext;

const COMPATIBLE_CLIENT_VERSION: &str = "1761720142";

fn device_info() -> String {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| String::from("user"));
    format!("{user}|Windows 10 Version 22H2")
}

/// Assemble an authenticated [`ApiContext`] from independent app state.
pub fn build_context() -> Result<ApiContext, String> {
    let cfg = config::read_config()?;
    if cfg.token.is_empty() {
        return Err(String::from(
            "no saved login token; sign in to IKUN PX first",
        ));
    }
    context_from_config(cfg)
}

pub fn build_login_context() -> Result<ApiContext, String> {
    let mut cfg = config::read_config()?;
    cfg.token.clear();
    context_from_config(cfg)
}

fn context_from_config(cfg: config::MarsConfig) -> Result<ApiContext, String> {
    let api_base = if cfg.api.is_empty() {
        String::from("http://47.111.161.57:8888")
    } else {
        cfg.api.clone()
    };
    Ok(ApiContext {
        api_base,
        token: cfg.token,
        email: cfg.email,
        device_id: cfg.device_id,
        device_info: device_info(),
        version: COMPATIBLE_CLIENT_VERSION.to_string(),
        platform: String::from("windows_64"),
        lang: String::from("hk"),
        code: String::from("Others"),
    })
}
