//! MarsLink control-plane API client.
//!
//! Every endpoint is a plain GET whose auth is just `token` in the query string
//! (no signature/timestamp), reverse-engineered by pointing the client's `api=`
//! at a logging proxy (see tools/capture-proxy.mjs). The node list comes back
//! AES-encrypted and is decrypted here.

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use super::crypto;

/// Everything needed to talk to the API, assembled from the ini + local defaults.
#[derive(Debug, Clone)]
pub struct ApiContext {
    pub api_base: String,
    pub token: String,
    pub email: String,
    pub device_id: String,
    pub device_info: String,
    pub version: String,
    pub platform: String,
    pub lang: String,
    pub code: String,
}

impl ApiContext {
    fn common_query(&self) -> Vec<(&'static str, String)> {
        vec![
            ("platform", self.platform.clone()),
            ("cur_version", self.version.clone()),
            ("token", self.token.clone()),
            ("deviceinfo", self.device_info.clone()),
            ("deviceid", self.device_id.clone()),
            ("lang", self.lang.clone()),
            ("code", self.code.clone()),
        ]
    }
}

#[derive(Debug, Deserialize)]
struct Envelope {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    #[serde(default)]
    pub uuid: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub transfer: String,
    #[serde(default)]
    pub charge_type: i64,
    #[serde(default)]
    pub charge_expire_date: i64,
    #[serde(default)]
    pub server_time: i64,
    #[serde(default)]
    pub payment_qr_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResult {
    pub email: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitConfig {
    #[serde(default)]
    pub sys_proxy_file: String,
    #[serde(default)]
    pub sys_local_file: String,
    #[serde(default)]
    pub country_file: String,
    #[serde(default)]
    pub public_msg_title: String,
    #[serde(default)]
    pub public_msg_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    #[serde(default)]
    pub api: Vec<String>,
    #[serde(default)]
    pub web: Vec<String>,
    #[serde(default)]
    pub msg: String,
}

const BOOTSTRAP_URLS: &[&str] = &[
    "http://ap.atyw.cn/greenday",
    "http://config.marslinkapp.com/greenday",
];

/// A node. Core fields are typed; the richer optional fields (geo, bandwidth,
/// flags) are kept loose because the server types them inconsistently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub uuid: String,
    pub name: String,
    pub addr: String,
    pub port: u16,
    #[serde(default)]
    pub random: bool,
    #[serde(default)]
    pub happy_wife: String,
    #[serde(default)]
    pub happy_life: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub free: serde_json::Value,
    #[serde(default)]
    pub udp: serde_json::Value,
    #[serde(default)]
    pub lat: serde_json::Value,
    #[serde(default)]
    pub lon: serde_json::Value,
    #[serde(default)]
    pub bandwidth_current: serde_json::Value,
    #[serde(default)]
    pub bandwidth_total: serde_json::Value,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("http client build failed: {e}"))
}

async fn get_envelope(
    ctx: &ApiContext,
    path: &str,
    extra: &[(&str, String)],
) -> Result<Envelope, String> {
    let url = format!("{}{}", ctx.api_base.trim_end_matches('/'), path);
    let mut query = ctx.common_query();
    query.extend(extra.iter().map(|(k, v)| (*k, v.clone())));

    let resp = client()?
        .get(&url)
        .query(&query)
        .send()
        .await
        .map_err(|e| format!("request to {path} failed: {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("reading {path} body failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("{path} returned HTTP {status}: {text}"));
    }
    serde_json::from_str::<Envelope>(&text)
        .map_err(|e| format!("{path} returned non-envelope JSON ({e}): {text}"))
}

async fn post_envelope(
    ctx: &ApiContext,
    path: &str,
    body: &serde_json::Value,
) -> Result<Envelope, String> {
    let url = format!("{}{}", ctx.api_base.trim_end_matches('/'), path);
    let response = client()?
        .post(&url)
        .query(&ctx.common_query())
        .json(body)
        .send()
        .await
        .map_err(|e| format!("request to {path} failed: {e}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("reading {path} body failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("{path} returned HTTP {status}: {text}"));
    }
    serde_json::from_str::<Envelope>(&text)
        .map_err(|e| format!("{path} returned non-envelope JSON ({e}): {text}"))
}

fn md5_hex(value: &str) -> String {
    let digest = Md5::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub async fn login(ctx: &ApiContext, email: &str, password: &str) -> Result<LoginResult, String> {
    if email.trim().is_empty() || password.len() < 6 {
        return Err(String::from(
            "email is required and password must have at least 6 characters",
        ));
    }
    let body = serde_json::json!({
        "email": email.trim(),
        "password": md5_hex(password),
        "platform": "windows_64",
        "captcha": "ignore",
        "captcha_id": "ignore"
    });
    let envelope = post_envelope(ctx, "/appuser/reglogin", &body).await?;
    if envelope.code != 0 {
        return Err(format!(
            "login failed (code {}): {}",
            envelope.code, envelope.msg
        ));
    }
    let token = envelope
        .data
        .get("token")
        .and_then(serde_json::Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| String::from("login response did not contain a token"))?;
    Ok(LoginResult {
        email: email.trim().to_string(),
        token: token.to_string(),
    })
}

pub async fn fetch_bootstrap() -> Result<BootstrapConfig, String> {
    let client = client()?;
    let mut errors = Vec::new();
    for url in BOOTSTRAP_URLS {
        let result = async {
            let response = client
                .get(*url)
                .send()
                .await
                .map_err(|e| format!("request failed: {e}"))?;
            if !response.status().is_success() {
                return Err(format!("HTTP {}", response.status()));
            }
            let encrypted = response
                .text()
                .await
                .map_err(|e| format!("read body: {e}"))?;
            let plain = crypto::decrypt_to_string(encrypted.trim())?;
            serde_json::from_str::<BootstrapConfig>(&plain)
                .map_err(|e| format!("parse decrypted bootstrap: {e}"))
        }
        .await;
        match result {
            Ok(config) if !config.api.is_empty() => return Ok(config),
            Ok(_) => errors.push(format!("{url}: empty API list")),
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }
    Err(format!(
        "all bootstrap endpoints failed: {}",
        errors.join("; ")
    ))
}

pub async fn user_info(ctx: &ApiContext) -> Result<UserInfo, String> {
    let env = get_envelope(ctx, "/appuser/userinfo", &[]).await?;
    if env.code != 0 {
        return Err(format!("userinfo failed (code {}): {}", env.code, env.msg));
    }
    serde_json::from_value(env.data).map_err(|e| format!("parse userinfo: {e}"))
}

pub async fn init_config(ctx: &ApiContext) -> Result<InitConfig, String> {
    let env = get_envelope(ctx, "/system/initconfig", &[]).await?;
    if env.code != 0 {
        return Err(format!(
            "initconfig failed (code {}): {}",
            env.code, env.msg
        ));
    }
    serde_json::from_value(env.data).map_err(|e| format!("parse initconfig: {e}"))
}

#[cfg(test)]
mod api_tests {
    use super::*;

    #[test]
    fn password_hash_matches_vendor_login() {
        assert_eq!(md5_hex("password"), "5f4dcc3b5aa765d61d8327deb882cf99");
    }
}

pub async fn node_list(ctx: &ApiContext) -> Result<Vec<Node>, String> {
    let env = get_envelope(
        ctx,
        "/system/clientnodelist",
        &[("email", ctx.email.clone())],
    )
    .await?;
    if env.code != 0 {
        return Err(format!("nodelist failed (code {}): {}", env.code, env.msg));
    }
    let cipher = env
        .data
        .as_str()
        .ok_or_else(|| String::from("nodelist data was not the expected encrypted string"))?;
    let plain = crypto::decrypt_to_string(cipher)?;
    serde_json::from_str::<Vec<Node>>(&plain).map_err(|e| {
        format!(
            "parse decrypted node list: {e}; head: {}",
            &plain.chars().take(120).collect::<String>()
        )
    })
}

/// Fetch and decrypt the raw node-list JSON (used by the standalone verifier).
pub async fn node_list_raw(ctx: &ApiContext) -> Result<String, String> {
    let env = get_envelope(
        ctx,
        "/system/clientnodelist",
        &[("email", ctx.email.clone())],
    )
    .await?;
    let cipher = env
        .data
        .as_str()
        .ok_or_else(|| String::from("nodelist data was not a string"))?;
    crypto::decrypt_to_string(cipher)
}
