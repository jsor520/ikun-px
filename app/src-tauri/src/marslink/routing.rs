use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::config;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const ADAPTER_NAME: &str = "ikun-px";
const TUN_ADDRESS: &str = "198.16.0.2";
const TUN_MASK: &str = "255.255.255.0";
const DNS_SERVER: &str = "114.114.114.114";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkState {
    pub active: bool,
    pub adapter: String,
    pub tun2socks_pid: u32,
    pub interface_index: u32,
}

fn state_path() -> PathBuf {
    config::app_dir().join("network-state.json")
}

fn read_state() -> Option<NetworkState> {
    serde_json::from_slice(&std::fs::read(state_path()).ok()?).ok()
}

async fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .await
        .map_err(|e| format!("launch {program}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub async fn adapter_index() -> Result<u32, String> {
    let script = format!(
        "(Get-NetAdapter -Name '{}' -ErrorAction Stop).ifIndex",
        ADAPTER_NAME
    );
    let output = run(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", &script],
    )
    .await?;
    output
        .lines()
        .last()
        .and_then(|line| line.trim().parse().ok())
        .ok_or_else(|| format!("could not parse {ADAPTER_NAME} interface index: {output}"))
}

pub async fn configure(interface_index: u32, tun2socks_pid: u32) -> Result<(), String> {
    run(
        "netsh",
        &[
            "interface",
            "ipv4",
            "set",
            "address",
            &format!("name={ADAPTER_NAME}"),
            "source=static",
            &format!("address={TUN_ADDRESS}"),
            &format!("mask={TUN_MASK}"),
            "gateway=none",
            "store=active",
        ],
    )
    .await?;
    // Wintun reports the adapter before Windows has finished installing the
    // connected/on-link route. Adding routes immediately can yield
    // WSAEHOSTUNREACH even though `route add` returns success.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let index = interface_index.to_string();
    run(
        "route",
        &[
            "ADD",
            DNS_SERVER,
            "MASK",
            "255.255.255.255",
            TUN_ADDRESS,
            "METRIC",
            "1",
            "IF",
            &index,
        ],
    )
    .await?;
    run(
        "route",
        &[
            "ADD",
            "44.0.0.0",
            "MASK",
            "255.0.0.0",
            TUN_ADDRESS,
            "METRIC",
            "1",
            "IF",
            &index,
        ],
    )
    .await?;
    run(
        "netsh",
        &[
            "interface",
            "ipv4",
            "set",
            "dnsservers",
            &format!("name={ADAPTER_NAME}"),
            "source=static",
            &format!("address={DNS_SERVER}"),
            "register=none",
            "validate=no",
        ],
    )
    .await?;
    let _ = run("ipconfig", &["/flushdns"]).await;

    let state = NetworkState {
        active: true,
        adapter: ADAPTER_NAME.to_string(),
        tun2socks_pid,
        interface_index,
    };
    std::fs::create_dir_all(config::app_dir())
        .map_err(|e| format!("create app data for network state: {e}"))?;
    std::fs::write(
        state_path(),
        serde_json::to_vec_pretty(&state).map_err(|e| format!("encode network state: {e}"))?,
    )
    .map_err(|e| format!("write network state: {e}"))
}

pub async fn cleanup() -> Result<(), String> {
    // Delete only the exact routes this app owns. Never reset the user's default
    // route or unrelated system-proxy configuration.
    let _ = run("route", &["DELETE", DNS_SERVER, "MASK", "255.255.255.255"]).await;
    let _ = run("route", &["DELETE", "44.0.0.0", "MASK", "255.0.0.0"]).await;
    let _ = run("ipconfig", &["/flushdns"]).await;
    if state_path().exists() {
        std::fs::remove_file(state_path())
            .map_err(|e| format!("remove network state marker: {e}"))?;
    }
    Ok(())
}

pub async fn recover_stale_state() -> Result<(), String> {
    if let Some(state) = read_state() {
        let script = format!(
            "$process = Get-Process -Id {} -ErrorAction SilentlyContinue; \
             if ($null -ne $process -and $process.ProcessName -eq 'tun2socks') {{ \
               Stop-Process -Id {} -Force -ErrorAction SilentlyContinue \
             }}",
            state.tun2socks_pid, state.tun2socks_pid
        );
        let _ = run(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", &script],
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    cleanup().await
}

pub fn has_stale_state() -> bool {
    state_path().is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_name_is_not_vendor_adapter() {
        assert_eq!(ADAPTER_NAME, "ikun-px");
        assert_ne!(ADAPTER_NAME, "wintun");
    }
}
