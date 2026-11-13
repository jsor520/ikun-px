//! Independent IKUN PX configuration and DPAPI-protected credentials.
//!
//! The legacy ini is read once for migration only. All subsequent writes go to
//! `%LOCALAPPDATA%\IKUN PX`, so developing this client cannot alter the running
//! vendor service.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

const DEFAULT_API: &str = "http://47.111.161.57:8888";
const DEFAULT_WEB: &str = "http://47.111.161.57:8889";
const MIGRATED_DEVICE_ID: &str = "f59c0177-c26b-4c1d-8ed7-55996e9f4d3d";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMode {
    Local,
    System,
    Tun,
}

impl CaptureMode {
    fn from_legacy_flags(proxy_mode: bool, tun_mode: bool) -> Self {
        if tun_mode {
            Self::Tun
        } else if proxy_mode {
            Self::System
        } else {
            Self::Local
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteMode {
    #[default]
    Smart,
    Global,
    Direct,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarsConfig {
    #[serde(skip_serializing)]
    pub token: String,
    pub email: String,
    pub last_select_node: String,
    pub capture_mode: CaptureMode,
    pub route_mode: RouteMode,
    pub api: String,
    pub web: String,
    pub device_id: String,
    pub password_saved: bool,
    pub logged_in: bool,
    pub git_proxy: bool,
    pub ssh_proxy: bool,
    pub auto_connect: bool,
    pub auto_start: bool,
    pub hotkeys_enabled: bool,
    pub show_node_icons: bool,
    pub sidebar_collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StoredConfig {
    email: String,
    last_select_node: String,
    proxy_mode: bool,
    tun_mode: bool,
    api: String,
    web: String,
    device_id: String,
    git_proxy: bool,
    ssh_proxy: bool,
    auto_connect: bool,
    auto_start: bool,
    hotkeys_enabled: bool,
    show_node_icons: bool,
    sidebar_collapsed: bool,
    route_mode: RouteMode,
    remember_password: bool,
}

impl Default for StoredConfig {
    fn default() -> Self {
        Self {
            email: String::new(),
            last_select_node: String::new(),
            proxy_mode: false,
            tun_mode: false,
            api: DEFAULT_API.to_string(),
            web: DEFAULT_WEB.to_string(),
            device_id: uuid::Uuid::new_v4().to_string(),
            git_proxy: false,
            ssh_proxy: false,
            auto_connect: false,
            auto_start: false,
            hotkeys_enabled: true,
            show_node_icons: false,
            sidebar_collapsed: false,
            route_mode: RouteMode::Smart,
            remember_password: true,
        }
    }
}

/// What the login form prefills itself with. The password only leaves the
/// DPAPI blob when the user asked for it to be remembered.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SavedCredentials {
    pub email: String,
    pub password: String,
    pub remember: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Credentials {
    token: String,
    password: Option<String>,
}

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn app_dir() -> PathBuf {
    local_app_data().join("IKUN PX")
}

fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

fn credentials_path() -> PathBuf {
    app_dir().join("credentials.dpapi")
}

fn legacy_ini_path() -> PathBuf {
    local_app_data()
        .join("MarsLinkProxy")
        .join("config_user.ini")
}

fn ensure_app_dir() -> Result<(), String> {
    std::fs::create_dir_all(app_dir()).map_err(|e| format!("create IKUN PX app data: {e}"))
}

fn legacy_value(raw: &str, key: &str) -> Option<String> {
    raw.lines().find_map(|line| {
        line.trim()
            .strip_prefix(key)
            .and_then(|rest| rest.trim_start().strip_prefix('='))
            .map(|value| value.trim().trim_matches('"').to_string())
    })
}

fn migrate_if_needed() -> Result<(), String> {
    if config_path().is_file() {
        return Ok(());
    }

    ensure_app_dir()?;
    let mut config = StoredConfig::default();
    if let Ok(raw) = std::fs::read_to_string(legacy_ini_path()) {
        config.email = legacy_value(&raw, "user_name").unwrap_or_default();
        config.last_select_node = legacy_value(&raw, "last_select_node").unwrap_or_default();
        config.proxy_mode = legacy_value(&raw, "proxy_mode").is_some_and(|v| v == "true");
        config.tun_mode = legacy_value(&raw, "tun_mode").is_some_and(|v| v == "true");
        config.api = legacy_value(&raw, "api").unwrap_or_else(|| DEFAULT_API.to_string());
        config.web = legacy_value(&raw, "web").unwrap_or_else(|| DEFAULT_WEB.to_string());
        config.device_id = MIGRATED_DEVICE_ID.to_string();

        let token = legacy_value(&raw, "token").unwrap_or_default();
        let password = legacy_value(&raw, "user_psw").filter(|value| !value.is_empty());
        if !token.is_empty() {
            write_credentials(&Credentials { token, password })?;
        }
    }
    write_stored(&config)
}

fn read_stored() -> Result<StoredConfig, String> {
    migrate_if_needed()?;
    let raw =
        std::fs::read_to_string(config_path()).map_err(|e| format!("read IKUN PX config: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse IKUN PX config: {e}"))
}

fn write_stored(config: &StoredConfig) -> Result<(), String> {
    ensure_app_dir()?;
    let json =
        serde_json::to_vec_pretty(config).map_err(|e| format!("encode IKUN PX config: {e}"))?;
    std::fs::write(config_path(), json).map_err(|e| format!("write IKUN PX config: {e}"))
}

fn dpapi_protect(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: plaintext
            .len()
            .try_into()
            .map_err(|_| String::from("credential data is too large"))?,
        pbData: plaintext.as_ptr() as *mut u8,
    };
    let mut output: CRYPT_INTEGER_BLOB = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(format!(
            "DPAPI protect failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let protected =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(protected)
}

fn dpapi_unprotect(protected: &[u8]) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: protected
            .len()
            .try_into()
            .map_err(|_| String::from("protected credential data is too large"))?,
        pbData: protected.as_ptr() as *mut u8,
    };
    let mut output: CRYPT_INTEGER_BLOB = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(format!(
            "DPAPI unprotect failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let plaintext =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(plaintext)
}

fn read_credentials() -> Result<Credentials, String> {
    let path = credentials_path();
    if !path.is_file() {
        return Ok(Credentials::default());
    }
    let protected = std::fs::read(path).map_err(|e| format!("read protected credentials: {e}"))?;
    let plaintext = dpapi_unprotect(&protected)?;
    serde_json::from_slice(&plaintext).map_err(|e| format!("parse protected credentials: {e}"))
}

fn write_credentials(credentials: &Credentials) -> Result<(), String> {
    ensure_app_dir()?;
    let plaintext =
        serde_json::to_vec(credentials).map_err(|e| format!("encode credentials: {e}"))?;
    let protected = dpapi_protect(&plaintext)?;
    std::fs::write(credentials_path(), protected)
        .map_err(|e| format!("write protected credentials: {e}"))
}

pub fn read_config() -> Result<MarsConfig, String> {
    let stored = read_stored()?;
    let credentials = read_credentials()?;
    let logged_in = !credentials.token.is_empty();
    Ok(MarsConfig {
        token: credentials.token,
        email: stored.email,
        last_select_node: stored.last_select_node,
        capture_mode: CaptureMode::from_legacy_flags(stored.proxy_mode, stored.tun_mode),
        route_mode: stored.route_mode,
        api: stored.api,
        web: stored.web,
        device_id: stored.device_id,
        password_saved: credentials.password.is_some(),
        logged_in,
        git_proxy: stored.git_proxy,
        ssh_proxy: stored.ssh_proxy,
        auto_connect: stored.auto_connect,
        auto_start: stored.auto_start,
        hotkeys_enabled: stored.hotkeys_enabled,
        show_node_icons: stored.show_node_icons,
        sidebar_collapsed: stored.sidebar_collapsed,
    })
}

pub fn saved_credentials() -> Result<SavedCredentials, String> {
    let stored = read_stored()?;
    let password = read_credentials()?.password.unwrap_or_default();
    Ok(SavedCredentials {
        email: stored.email,
        password,
        remember: stored.remember_password,
    })
}

pub fn save_login(email: &str, token: &str, password: Option<&str>) -> Result<MarsConfig, String> {
    let mut stored = read_stored()?;
    stored.email = email.to_string();
    stored.remember_password = password.is_some();
    write_stored(&stored)?;
    write_credentials(&Credentials {
        token: token.to_string(),
        password: password.map(ToString::to_string),
    })?;
    read_config()
}

/// Ends the session by dropping the token, which is what `logged_in` is derived
/// from. The remembered email and password survive so the login form can
/// prefill them; `forget` wipes them for good.
pub fn clear_login(forget: bool) -> Result<(), String> {
    let mut stored = read_stored()?;
    let keep = (!forget && stored.remember_password)
        .then(read_credentials)
        .transpose()?
        .and_then(|credentials| credentials.password)
        .filter(|password| !password.is_empty());

    match keep {
        Some(password) => write_credentials(&Credentials {
            token: String::new(),
            password: Some(password),
        })?,
        None if credentials_path().exists() => std::fs::remove_file(credentials_path())
            .map_err(|e| format!("remove protected credentials: {e}"))?,
        None => {}
    }

    if forget {
        stored.email.clear();
        stored.remember_password = StoredConfig::default().remember_password;
        write_stored(&stored)?;
    }
    Ok(())
}

pub fn set_selected_node(uuid: &str) -> Result<(), String> {
    let mut stored = read_stored()?;
    stored.last_select_node = uuid.to_string();
    write_stored(&stored)
}

pub fn set_network_modes(capture_mode: CaptureMode, route_mode: RouteMode) -> Result<(), String> {
    let mut stored = read_stored()?;
    stored.proxy_mode = capture_mode == CaptureMode::System;
    stored.tun_mode = capture_mode == CaptureMode::Tun;
    stored.route_mode = route_mode;
    write_stored(&stored)
}

pub fn set_integrations(
    git_proxy: bool,
    ssh_proxy: bool,
    auto_connect: bool,
    auto_start: bool,
    hotkeys_enabled: bool,
) -> Result<(), String> {
    let mut stored = read_stored()?;
    stored.git_proxy = git_proxy;
    stored.ssh_proxy = ssh_proxy;
    stored.auto_connect = auto_connect;
    stored.auto_start = auto_start;
    stored.hotkeys_enabled = hotkeys_enabled;
    write_stored(&stored)
}

pub fn set_ui_preferences(show_node_icons: bool, sidebar_collapsed: bool) -> Result<(), String> {
    let mut stored = read_stored()?;
    stored.show_node_icons = show_node_icons;
    stored.sidebar_collapsed = sidebar_collapsed;
    write_stored(&stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_round_trip() {
        let secret = b"synthetic-test-credential";
        let protected = dpapi_protect(secret).unwrap();
        assert_ne!(protected, secret);
        assert_eq!(dpapi_unprotect(&protected).unwrap(), secret);
    }

    #[test]
    fn legacy_capture_flags_map_to_one_capture_mode() {
        assert_eq!(
            CaptureMode::from_legacy_flags(false, false),
            CaptureMode::Local
        );
        assert_eq!(
            CaptureMode::from_legacy_flags(true, false),
            CaptureMode::System
        );
        assert_eq!(
            CaptureMode::from_legacy_flags(false, true),
            CaptureMode::Tun
        );
        assert_eq!(CaptureMode::from_legacy_flags(true, true), CaptureMode::Tun);
    }

    #[test]
    fn old_json_defaults_to_smart_routing() {
        let stored: StoredConfig =
            serde_json::from_str(r#"{"proxy_mode":true,"tun_mode":false}"#).unwrap();
        assert_eq!(stored.route_mode, RouteMode::Smart);
        assert_eq!(
            CaptureMode::from_legacy_flags(stored.proxy_mode, stored.tun_mode),
            CaptureMode::System
        );
    }

    #[test]
    fn config_written_before_remember_opts_into_prefill() {
        let stored: StoredConfig = serde_json::from_str(r#"{"email":"a@b.c"}"#).unwrap();
        assert!(stored.remember_password);
    }
}
