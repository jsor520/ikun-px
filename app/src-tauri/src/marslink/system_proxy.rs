use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use super::config;

const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
const INTERNET_OPTION_REFRESH: u32 = 37;
const INTERNET_OPTION_SETTINGS_CHANGED: u32 = 39;

#[link(name = "wininet")]
unsafe extern "system" {
    fn InternetSetOptionW(
        handle: *mut std::ffi::c_void,
        option: u32,
        buffer: *mut u8,
        length: u32,
    ) -> i32;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProxySnapshot {
    proxy_enable: u32,
    proxy_server: Option<String>,
    proxy_override: Option<String>,
    auto_config_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemProxyStatus {
    pub enabled: bool,
    pub server: String,
    pub listener_alive: bool,
}

fn snapshot_path() -> PathBuf {
    config::app_dir().join("system-proxy-snapshot.json")
}

fn settings_key() -> Result<RegKey, String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            INTERNET_SETTINGS,
            winreg::enums::KEY_READ | winreg::enums::KEY_WRITE,
        )
        .map_err(|e| format!("open Windows Internet Settings: {e}"))
}

fn optional_string(key: &RegKey, name: &str) -> Option<String> {
    key.get_value(name).ok()
}

fn capture() -> Result<ProxySnapshot, String> {
    let key = settings_key()?;
    Ok(ProxySnapshot {
        proxy_enable: key.get_value("ProxyEnable").unwrap_or(0),
        proxy_server: optional_string(&key, "ProxyServer"),
        proxy_override: optional_string(&key, "ProxyOverride"),
        auto_config_url: optional_string(&key, "AutoConfigURL"),
    })
}

pub fn status() -> Result<SystemProxyStatus, String> {
    let key = settings_key()?;
    let enabled = key.get_value::<u32, _>("ProxyEnable").unwrap_or(0) != 0;
    let server: String = key.get_value("ProxyServer").unwrap_or_default();
    let listener_alive = loopback_proxy_port(&server).is_some_and(|port| {
        std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(300),
        )
        .is_ok()
    });
    Ok(SystemProxyStatus {
        enabled,
        server,
        listener_alive,
    })
}

fn notify_windows() {
    unsafe {
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null_mut(),
            0,
        );
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null_mut(),
            0,
        );
    }
}

fn set_optional(key: &RegKey, name: &str, value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value {
        key.set_value(name, &value)
            .map_err(|e| format!("set {name}: {e}"))
    } else {
        match key.delete_value(name) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("delete {name}: {error}")),
        }
    }
}

pub fn enable(http_port: u16) -> Result<(), String> {
    if !snapshot_path().is_file() {
        std::fs::create_dir_all(config::app_dir())
            .map_err(|e| format!("create app data for proxy snapshot: {e}"))?;
        let snapshot = capture()?;
        std::fs::write(
            snapshot_path(),
            serde_json::to_vec_pretty(&snapshot)
                .map_err(|e| format!("encode proxy snapshot: {e}"))?,
        )
        .map_err(|e| format!("write proxy snapshot: {e}"))?;
    }

    let key = settings_key()?;
    key.set_value("ProxyEnable", &1u32)
        .map_err(|e| format!("enable system proxy: {e}"))?;
    key.set_value("ProxyServer", &format!("127.0.0.1:{http_port}"))
        .map_err(|e| format!("set system proxy address: {e}"))?;
    key.set_value("ProxyOverride", &"<local>")
        .map_err(|e| format!("set system proxy bypass: {e}"))?;
    set_optional(&key, "AutoConfigURL", None)?;
    notify_windows();
    Ok(())
}

pub fn restore() -> Result<(), String> {
    if !snapshot_path().is_file() {
        return Ok(());
    }
    let snapshot: ProxySnapshot = serde_json::from_slice(
        &std::fs::read(snapshot_path()).map_err(|e| format!("read proxy snapshot: {e}"))?,
    )
    .map_err(|e| format!("parse proxy snapshot: {e}"))?;
    let key = settings_key()?;
    key.set_value("ProxyEnable", &snapshot.proxy_enable)
        .map_err(|e| format!("restore ProxyEnable: {e}"))?;
    set_optional(&key, "ProxyServer", snapshot.proxy_server.as_deref())?;
    set_optional(&key, "ProxyOverride", snapshot.proxy_override.as_deref())?;
    set_optional(&key, "AutoConfigURL", snapshot.auto_config_url.as_deref())?;
    notify_windows();
    std::fs::remove_file(snapshot_path()).map_err(|e| format!("remove proxy snapshot: {e}"))
}

pub fn has_snapshot() -> bool {
    snapshot_path().is_file()
}

/// Disable a stale loopback proxy only when its port has no listener. This is
/// deliberately narrower than resetting all Internet Settings.
pub fn repair_stale_loopback_proxy() -> Result<bool, String> {
    let key = settings_key()?;
    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    let server: String = key.get_value("ProxyServer").unwrap_or_default();
    if enabled == 0 {
        return Ok(false);
    }
    let Some(port) = loopback_proxy_port(&server) else {
        return Ok(false);
    };
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    if std::net::TcpStream::connect_timeout(&address, Duration::from_millis(400)).is_ok() {
        return Ok(false);
    }
    key.set_value("ProxyEnable", &0u32)
        .map_err(|e| format!("disable stale loopback proxy: {e}"))?;
    notify_windows();
    Ok(true)
}

fn loopback_proxy_port(server: &str) -> Option<u16> {
    server
        .split(';')
        .filter_map(|entry| entry.split('=').next_back())
        .map(str::trim)
        .find_map(|address| {
            address
                .strip_prefix("127.0.0.1:")
                .and_then(|port| port.parse().ok())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_and_per_protocol_loopback_proxy() {
        assert_eq!(loopback_proxy_port("127.0.0.1:11888"), Some(11888));
        assert_eq!(
            loopback_proxy_port("http=127.0.0.1:8000;https=127.0.0.1:8001"),
            Some(8000)
        );
        assert_eq!(loopback_proxy_port("proxy.example:8080"), None);
    }
}
