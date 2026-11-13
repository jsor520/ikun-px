//! Hard-gate interoperability probe.
//!
//! This executable opens a MarsLink node directly through the independently
//! implemented wire protocol. It never starts or talks to MarsLinkProxy.exe.

use std::time::Duration;

use ikun_px_lib::marslink;
use ikun_px_lib::marslink::api;
use ikun_px_lib::marslink::config;
use ikun_px_lib::marslink::transport::{Destination, MarsStream};
use tokio::time::timeout;

#[tokio::main]
async fn main() {
    if let Err(error) = probe().await {
        eprintln!("PROTOCOL_GATE_FAILED: {error}");
        std::process::exit(1);
    }
}

async fn probe() -> Result<(), String> {
    let config = config::read_config()?;
    let context = marslink::build_context()?;
    let nodes = api::node_list(&context).await?;
    let node = nodes
        .iter()
        .find(|node| node.uuid == config.last_select_node)
        .or_else(|| nodes.first())
        .ok_or_else(|| String::from("account has no available nodes"))?;

    println!(
        "Connecting directly to {} ({}:{}) without MarsLinkProxy.exe",
        node.name, node.addr, node.port
    );

    let mut stream = MarsStream::connect(
        node,
        &context.token,
        Destination::domain("api.ipify.org", 80),
    )
    .await?;
    stream
        .write_all(
            b"GET / HTTP/1.1\r\n\
              Host: api.ipify.org\r\n\
              User-Agent: ikun-px-protocol-probe/1\r\n\
              Accept: text/plain\r\n\
              Connection: close\r\n\
              \r\n",
        )
        .await?;

    let mut response = Vec::new();
    for _ in 0..32 {
        match timeout(Duration::from_secs(10), stream.read_chunk()).await {
            Ok(Ok(chunk)) => {
                response.extend_from_slice(&chunk);
                if response.len() > 64 * 1024 {
                    break;
                }
            }
            Ok(Err(error)) if !response.is_empty() && error.contains("closed") => break,
            Ok(Err(error)) => return Err(error),
            Err(_) if !response.is_empty() => break,
            Err(_) => return Err(String::from("timed out waiting for proxied HTTP response")),
        }
    }

    let text = String::from_utf8_lossy(&response);
    if !text.starts_with("HTTP/1.") {
        return Err(format!(
            "unexpected proxied response ({} bytes): {:?}",
            response.len(),
            text.chars().take(160).collect::<String>()
        ));
    }
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").trim();
    if body.parse::<std::net::IpAddr>().is_err() {
        return Err(format!("IP echo returned unexpected body: {body:?}"));
    }

    println!("PROTOCOL_GATE_PASSED");
    println!("Selected node: {}", node.name);
    println!("Observed exit IP: {body}");
    Ok(())
}
