//! Isolated TUN smoke test that coexists with the legacy service.
//! It uses a dedicated adapter, subnet and /32 fake-IP route.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ikun_px_lib::marslink;
use ikun_px_lib::marslink::api;
use ikun_px_lib::marslink::config;
use ikun_px_lib::marslink::fake_dns::FakeDns;
use ikun_px_lib::marslink::rules::RuleEngine;
use ikun_px_lib::marslink::runtime::RuntimeManager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const HTTP_PORT: u16 = 21888;
const SOCKS_PORT: u16 = 21889;
const ADAPTER: &str = "ikun-px-test";
const DEVICE: &str = "tun://ikun-px-test?GUID={6B23E999-A36B-4514-85E8-3B6659D2FA52}";
const FAKE_IP: Ipv4Addr = Ipv4Addr::new(44, 0, 0, 1);

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("TUN_PROBE_FAILED: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let config = config::read_config()?;
    let context = marslink::build_context()?;
    let node = api::node_list(&context)
        .await?
        .into_iter()
        .find(|node| node.uuid == config.last_select_node)
        .ok_or_else(|| String::from("selected node is unavailable"))?;

    let fake_dns = Arc::new(FakeDns::default());
    assert_eq!(fake_dns.allocate("api.ipify.org"), FAKE_IP);
    let runtime = RuntimeManager::new();
    runtime
        .start_with_rules(
            node,
            context.token,
            HTTP_PORT,
            SOCKS_PORT,
            Arc::new(RuleEngine::default()),
            fake_dns,
        )
        .await?;

    let mut child = start_tun2socks()?;
    let result = async {
        let index = wait_adapter(&mut child).await?;
        configure_test_adapter(index).await?;
        sleep(Duration::from_secs(2)).await;
        add_test_route(index).await?;
        sleep(Duration::from_secs(2)).await;
        request_through_tun().await
    }
    .await;

    delete_test_route().await;
    let _ = child.start_kill();
    let _ = timeout(Duration::from_secs(5), child.wait()).await;
    remove_test_adapter().await;
    let _ = runtime.stop().await;

    let body = result?;
    println!("TUN_PROBE_PASSED: {body}");
    Ok(())
}

fn sidecar() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("tun")
        .join("tun2socks.exe")
}

fn start_tun2socks() -> Result<Child, String> {
    let executable = sidecar();
    Command::new(&executable)
        .current_dir(executable.parent().expect("sidecar parent"))
        .args([
            "-device",
            DEVICE,
            "-proxy",
            &format!("socks5://127.0.0.1:{SOCKS_PORT}"),
            "-loglevel",
            "silent",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("start isolated tun2socks: {e}"))
}

async fn hidden(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .await
        .map_err(|e| format!("launch {program}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

async fn wait_adapter(child: &mut Child) -> Result<u32, String> {
    for _ in 0..60 {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("test tun2socks exited: {status}"));
        }
        let script = format!("(Get-NetAdapter -Name '{ADAPTER}' -ErrorAction Stop).ifIndex");
        if let Ok(output) = hidden(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", &script],
        )
        .await
        {
            if let Ok(index) = output.trim().parse() {
                return Ok(index);
            }
        }
        sleep(Duration::from_millis(250)).await;
    }
    Err(String::from("test Wintun adapter did not appear"))
}

async fn configure_test_adapter(index: u32) -> Result<(), String> {
    hidden(
        "netsh",
        &[
            "interface",
            "ipv4",
            "set",
            "address",
            &format!("name={ADAPTER}"),
            "source=static",
            "address=198.17.0.2",
            "mask=255.255.255.0",
            "gateway=none",
            "store=active",
        ],
    )
    .await?;
    let _ = index;
    Ok(())
}

async fn add_test_route(index: u32) -> Result<(), String> {
    hidden(
        "route",
        &[
            "ADD",
            "44.0.0.1",
            "MASK",
            "255.255.255.255",
            "198.17.0.2",
            "METRIC",
            "1",
            "IF",
            &index.to_string(),
        ],
    )
    .await
    .map(|_| ())
}

async fn delete_test_route() {
    let _ = hidden("route", &["DELETE", "44.0.0.1", "MASK", "255.255.255.255"]).await;
}

async fn remove_test_adapter() {
    let script = format!(
        "$a=Get-NetAdapter -Name '{ADAPTER}' -ErrorAction SilentlyContinue;\
         if($a){{pnputil /remove-device $a.PnPDeviceID | Out-Null}}"
    );
    let _ = hidden(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", &script],
    )
    .await;
}

async fn request_through_tun() -> Result<String, String> {
    let address = SocketAddr::new(IpAddr::V4(FAKE_IP), 80);
    let mut stream = timeout(Duration::from_secs(10), TcpStream::connect(address))
        .await
        .map_err(|_| String::from("TUN TCP connect timed out"))?
        .map_err(|e| format!("TUN TCP connect: {e}"))?;
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: api.ipify.org\r\nConnection: close\r\n\r\n")
        .await
        .map_err(|e| format!("write through TUN: {e}"))?;
    let mut response = Vec::new();
    timeout(Duration::from_secs(20), stream.read_to_end(&mut response))
        .await
        .map_err(|_| String::from("TUN HTTP response timed out"))?
        .map_err(|e| format!("read through TUN: {e}"))?;
    let text = String::from_utf8_lossy(&response);
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").trim();
    body.parse::<IpAddr>()
        .map_err(|_| format!("unexpected TUN response: {text:?}"))?;
    Ok(body.to_string())
}
