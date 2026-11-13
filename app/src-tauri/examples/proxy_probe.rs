//! Side-by-side integration test for the independent local proxies.
//! Uses 21888/21889 so the currently running vendor service remains untouched.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use ikun_px_lib::marslink;
use ikun_px_lib::marslink::api;
use ikun_px_lib::marslink::config;
use ikun_px_lib::marslink::diagnostics;
use ikun_px_lib::marslink::fake_dns::FakeDns;
use ikun_px_lib::marslink::rules::{RouteDecision, RuleEngine};
use ikun_px_lib::marslink::runtime::RuntimeManager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::time::timeout;

const HTTP_PORT: u16 = 21888;
const SOCKS_PORT: u16 = 21889;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("PROXY_PROBE_FAILED: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let config = config::read_config()?;
    let context = marslink::build_context()?;
    let nodes = api::node_list(&context).await?;
    let node = nodes
        .into_iter()
        .find(|node| node.uuid == config.last_select_node)
        .ok_or_else(|| String::from("selected node is unavailable"))?;

    let runtime = RuntimeManager::new();
    runtime
        .start(node.clone(), context.token.clone(), HTTP_PORT, SOCKS_PORT)
        .await?;
    println!("Independent proxies started on 127.0.0.1:{HTTP_PORT} and 127.0.0.1:{SOCKS_PORT}");

    let result = async {
        let socks_response = request_via_socks().await?;
        require_http_response("SOCKS5", &socks_response)?;
        println!("SOCKS5_PROBE_PASSED");

        let http_response = request_via_http_proxy().await?;
        require_http_response("HTTP", &http_response)?;
        println!("HTTP_PROXY_PROBE_PASSED");

        let fake_ip = request_fake_dns().await?;
        println!("FAKE_DNS_PROBE_PASSED: api.ipify.org -> {fake_ip}");
        let fake_response = request_via_fake_ip(fake_ip).await?;
        require_http_response("FAKE-IP", &fake_response)?;
        println!("FAKE_IP_REVERSE_MAPPING_PASSED");

        check_diagnostic_probes(&node).await?;

        let status = runtime.status().await;
        println!(
            "Traffic counters: upload={} download={}",
            status.upload_bytes, status.download_bytes
        );
        Ok::<(), String>(())
    }
    .await;

    let stop_result = runtime.stop().await;
    result?;
    stop_result?;
    println!("PROXY_RUNTIME_STOPPED_CLEANLY");

    let direct_runtime = RuntimeManager::new();
    let direct_rules = RuleEngine::from_text("", "", "", r"^example\.com$", RouteDecision::Proxy);
    direct_runtime
        .start_with_rules(
            node.clone(),
            context.token,
            HTTP_PORT,
            SOCKS_PORT,
            Arc::new(direct_rules),
            Arc::new(FakeDns::default()),
        )
        .await?;
    let direct_response = request_example_via_http_proxy().await?;
    direct_runtime.stop().await?;
    if !String::from_utf8_lossy(&direct_response).contains("Example Domain") {
        return Err(String::from(
            "direct rule did not return the expected example.com response",
        ));
    }
    println!("DIRECT_RULE_PROBE_PASSED");
    println!("Node: {}", node.name);
    Ok(())
}

/// Same probes the 网络诊断 button runs, against the live tunnel rather than
/// the app's ports.
async fn check_diagnostic_probes(node: &api::Node) -> Result<(), String> {
    let address = diagnostics::node_probe(node).await;
    if !address.ok || address.latency_ms.is_none() {
        return Err(format!("node probe failed: {}", address.detail));
    }
    println!(
        "NODE_PROBE_PASSED: {} {} ms ({})",
        address.target,
        address.latency_ms.unwrap_or_default(),
        address.detail
    );

    let web = diagnostics::web_probe(HTTP_PORT).await;
    if !web.ok || web.latency_ms.is_none() {
        return Err(format!("web probe failed: {}", web.detail));
    }
    let speed = web
        .speed_kbps
        .ok_or_else(|| format!("web probe reported no speed: {}", web.detail))?;
    println!(
        "WEB_PROBE_PASSED: {} opened in {} ms at {speed} KB/s ({})",
        web.target,
        web.latency_ms.unwrap_or_default(),
        web.detail
    );
    Ok(())
}

async fn request_via_socks() -> Result<Vec<u8>, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", SOCKS_PORT))
        .await
        .map_err(|e| format!("connect independent SOCKS5: {e}"))?;
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| format!("write SOCKS greeting: {e}"))?;
    let mut greeting = [0u8; 2];
    stream
        .read_exact(&mut greeting)
        .await
        .map_err(|e| format!("read SOCKS greeting: {e}"))?;
    if greeting != [0x05, 0x00] {
        return Err(format!("unexpected SOCKS auth response {greeting:02x?}"));
    }

    let host = b"api.ipify.org";
    let mut request = vec![0x05, 0x01, 0x00, 0x03, host.len() as u8];
    request.extend_from_slice(host);
    request.extend_from_slice(&80u16.to_be_bytes());
    stream
        .write_all(&request)
        .await
        .map_err(|e| format!("write SOCKS CONNECT: {e}"))?;
    let mut reply = [0u8; 10];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| format!("read SOCKS CONNECT: {e}"))?;
    if reply[1] != 0 {
        return Err(format!("SOCKS CONNECT failed with code {}", reply[1]));
    }
    stream
        .write_all(ip_echo_request())
        .await
        .map_err(|e| format!("write request through SOCKS: {e}"))?;
    read_response(&mut stream).await
}

async fn request_via_http_proxy() -> Result<Vec<u8>, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", HTTP_PORT))
        .await
        .map_err(|e| format!("connect independent HTTP proxy: {e}"))?;
    stream
        .write_all(
            b"GET http://api.ipify.org/ HTTP/1.1\r\n\
              Host: api.ipify.org\r\n\
              Connection: close\r\n\
              \r\n",
        )
        .await
        .map_err(|e| format!("write request through HTTP proxy: {e}"))?;
    read_response(&mut stream).await
}

async fn request_example_via_http_proxy() -> Result<Vec<u8>, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", HTTP_PORT))
        .await
        .map_err(|e| format!("connect direct-rule HTTP proxy: {e}"))?;
    stream
        .write_all(
            b"GET http://example.com/ HTTP/1.1\r\n\
              Host: example.com\r\n\
              Connection: close\r\n\
              \r\n",
        )
        .await
        .map_err(|e| format!("write direct-rule request: {e}"))?;
    read_response(&mut stream).await
}

async fn request_fake_dns() -> Result<Ipv4Addr, String> {
    let mut control = TcpStream::connect(("127.0.0.1", SOCKS_PORT))
        .await
        .map_err(|e| format!("connect SOCKS5 UDP control: {e}"))?;
    control
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| format!("write UDP auth greeting: {e}"))?;
    let mut auth = [0u8; 2];
    control
        .read_exact(&mut auth)
        .await
        .map_err(|e| format!("read UDP auth greeting: {e}"))?;
    control
        .write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|e| format!("write UDP ASSOCIATE: {e}"))?;
    let mut reply = [0u8; 10];
    control
        .read_exact(&mut reply)
        .await
        .map_err(|e| format!("read UDP ASSOCIATE: {e}"))?;
    if reply[1] != 0 || reply[3] != 1 {
        return Err(format!("UDP ASSOCIATE failed: {reply:02x?}"));
    }
    let relay_port = u16::from_be_bytes([reply[8], reply[9]]);
    let udp = UdpSocket::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("bind UDP probe socket: {e}"))?;
    let mut packet = vec![0, 0, 0, 1, 114, 114, 114, 114, 0, 53];
    packet.extend_from_slice(&dns_a_query("api.ipify.org"));
    udp.send_to(&packet, ("127.0.0.1", relay_port))
        .await
        .map_err(|e| format!("send fake DNS query: {e}"))?;
    let mut response = [0u8; 2048];
    let (count, _) = timeout(Duration::from_secs(5), udp.recv_from(&mut response))
        .await
        .map_err(|_| String::from("timed out waiting for fake DNS response"))?
        .map_err(|e| format!("read fake DNS response: {e}"))?;
    if count < 14 {
        return Err(String::from("fake DNS response is too short"));
    }
    Ok(Ipv4Addr::new(
        response[count - 4],
        response[count - 3],
        response[count - 2],
        response[count - 1],
    ))
}

async fn request_via_fake_ip(fake_ip: Ipv4Addr) -> Result<Vec<u8>, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", SOCKS_PORT))
        .await
        .map_err(|e| format!("connect SOCKS5 for fake IP: {e}"))?;
    stream
        .write_all(&[5, 1, 0])
        .await
        .map_err(|e| e.to_string())?;
    let mut auth = [0u8; 2];
    stream
        .read_exact(&mut auth)
        .await
        .map_err(|e| e.to_string())?;
    let mut request = vec![5, 1, 0, 1];
    request.extend_from_slice(&fake_ip.octets());
    request.extend_from_slice(&80u16.to_be_bytes());
    stream
        .write_all(&request)
        .await
        .map_err(|e| format!("write fake-IP CONNECT: {e}"))?;
    let mut reply = [0u8; 10];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|e| format!("read fake-IP CONNECT: {e}"))?;
    if reply[1] != 0 {
        return Err(format!("fake-IP CONNECT failed with code {}", reply[1]));
    }
    stream
        .write_all(ip_echo_request())
        .await
        .map_err(|e| format!("write through fake-IP mapping: {e}"))?;
    read_response(&mut stream).await
}

fn dns_a_query(domain: &str) -> Vec<u8> {
    let mut packet = vec![
        0x51, 0x50, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    for label in domain.split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.extend_from_slice(&[0, 0, 1, 0, 1]);
    packet
}

fn ip_echo_request() -> &'static [u8] {
    b"GET / HTTP/1.1\r\n\
      Host: api.ipify.org\r\n\
      Connection: close\r\n\
      \r\n"
}

async fn read_response(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    timeout(Duration::from_secs(20), stream.read_to_end(&mut output))
        .await
        .map_err(|_| String::from("timed out reading local proxy response"))?
        .map_err(|e| format!("read local proxy response: {e}"))?;
    Ok(output)
}

fn require_http_response(kind: &str, response: &[u8]) -> Result<std::net::IpAddr, String> {
    let text = String::from_utf8_lossy(response);
    if !text.starts_with("HTTP/1.") {
        return Err(format!("{kind} returned unexpected data: {text:?}"));
    }
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").trim();
    let address = body
        .parse::<std::net::IpAddr>()
        .map_err(|_| format!("{kind} returned invalid IP body: {body:?}"))?;
    println!("{kind} exit IP: {address}");
    Ok(address)
}
