//! Active probes behind the 网络诊断 button: how fast the node we are connected
//! to answers, and how fast a page loads through our own local proxy.

use std::time::{Duration, Instant};

use serde::Serialize;

use super::api::Node;

const HANDSHAKE_ATTEMPTS: u8 = 3;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Stands in for opening a page: a tiny well-known endpoint, so the number is
/// the round trip through the tunnel rather than someone else's page weight.
const PAGE_URL: &str = "https://www.gstatic.com/generate_204";
/// Sized downloads for the rate, tried in order: every sample is larger than
/// the window can drain so a fast line is never capped, and exit nodes sit in
/// datacenter ranges that some speed-test hosts refuse outright.
const SAMPLE_URLS: &[&str] = &[
    "https://speed.cloudflare.com/__down?bytes=104857600",
    "https://proof.ovh.net/files/100Mb.dat",
    "https://speed.hetzner.de/100MB.bin",
];
/// A slow line must not hold the button hostage: the rate is whatever arrived
/// inside this window, and every sample above is far larger than even a fast
/// line drains in that time.
const SAMPLE_WINDOW: Duration = Duration::from_secs(5);
/// A sample that finishes this small never came from the test file it asked
/// for, so it is a broken host rather than a measurement.
const MIN_SAMPLE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    /// What was measured: `host:port` for the node, the host for the page.
    pub target: String,
    pub ok: bool,
    pub latency_ms: Option<u64>,
    /// Only the page probe transfers enough bytes to report a rate.
    pub speed_kbps: Option<u64>,
    pub detail: String,
}

/// TCP handshake against the address the tunnel itself is using.
pub async fn node_probe(node: &Node) -> Probe {
    let mut samples = Vec::new();
    let mut failure = String::from("无法建立连接");
    for _ in 0..HANDSHAKE_ATTEMPTS {
        let started = Instant::now();
        let attempt = tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            tokio::net::TcpStream::connect((node.addr.as_str(), node.port)),
        )
        .await;
        match attempt {
            Ok(Ok(_)) => samples.push(started.elapsed().as_millis() as u64),
            Ok(Err(error)) => failure = error.to_string(),
            Err(_) => failure = format!("{} 秒内未完成握手", HANDSHAKE_TIMEOUT.as_secs()),
        }
    }

    let latency =
        (!samples.is_empty()).then(|| samples.iter().sum::<u64>() / samples.len() as u64);
    Probe {
        target: format!("{}:{}", node.addr, node.port),
        ok: latency.is_some(),
        latency_ms: latency,
        speed_kbps: None,
        detail: match latency {
            Some(_) => format!(
                "{} · {}/{HANDSHAKE_ATTEMPTS} 次握手成功",
                node.name,
                samples.len()
            ),
            None => format!("{} · 握手失败：{failure}", node.name),
        },
    }
}

/// Load a page and pull a sample download through the local HTTP proxy, so the
/// result covers the whole chain a browser takes: local listener, routing
/// rules and node. The rate is reported separately because a line can open
/// pages promptly and still crawl on transfers.
pub async fn web_probe(http_port: u16) -> Probe {
    let client = match client(http_port) {
        Ok(client) => client,
        Err(error) => return failed_probe(error),
    };

    let opened = match page_latency(&client).await {
        Ok(elapsed) => elapsed,
        Err(error) => return failed_probe(error),
    };
    let sample = sample_download(&client).await;

    Probe {
        target: host_of(PAGE_URL),
        ok: true,
        latency_ms: Some(opened.as_millis() as u64),
        speed_kbps: sample
            .as_ref()
            .ok()
            .and_then(|sample| throughput_kbps(sample.bytes, sample.elapsed)),
        detail: match sample {
            Ok(sample) => format!(
                "下载 {} 用时 {:.1} 秒（{}）",
                format_bytes(sample.bytes),
                sample.elapsed.as_secs_f64(),
                host_of(sample.url)
            ),
            Err(error) => format!("仅测得打开耗时（测速失败：{error}）"),
        },
    }
}

fn client(http_port: u16) -> Result<reqwest::Client, String> {
    let proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{http_port}"))
        .map_err(|e| format!("本地代理地址无效：{e}"))?;
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0")
        .proxy(proxy)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("创建诊断客户端失败：{e}"))
}

async fn page_latency(client: &reqwest::Client) -> Result<Duration, String> {
    let started = Instant::now();
    let response = client
        .get(PAGE_URL)
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("返回 HTTP {status}"));
    }
    response
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败：{e}"))?;
    Ok(started.elapsed())
}

struct Sample {
    url: &'static str,
    bytes: u64,
    elapsed: Duration,
}

async fn sample_download(client: &reqwest::Client) -> Result<Sample, String> {
    let mut failure = String::from("没有可用的测速地址");
    for url in SAMPLE_URLS {
        match download_window(client, url).await {
            Ok(sample) => return Ok(sample),
            Err(error) => failure = format!("{} {error}", host_of(url)),
        }
    }
    Err(failure)
}

/// Reads chunks until the sample ends or the window closes, and times only the
/// body so the handshake does not drag the rate down.
async fn download_window(client: &reqwest::Client, url: &'static str) -> Result<Sample, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("返回 HTTP {status}"));
    }

    let started = Instant::now();
    let mut bytes = 0u64;
    let mut complete = false;
    loop {
        let Some(remaining) = SAMPLE_WINDOW.checked_sub(started.elapsed()) else {
            break;
        };
        match tokio::time::timeout(remaining, response.chunk()).await {
            Ok(Ok(Some(chunk))) => bytes += chunk.len() as u64,
            Ok(Ok(None)) => {
                complete = true;
                break;
            }
            Ok(Err(error)) => return Err(format!("读取响应失败：{error}")),
            Err(_) => break,
        }
    }
    if !sample_is_usable(bytes, complete) {
        return Err(format!("测速文件只有 {bytes} 字节"));
    }
    Ok(Sample {
        url,
        bytes,
        elapsed: started.elapsed(),
    })
}

/// A host that answers 200 with a placeholder instead of its test file would
/// otherwise win the candidate list and report a meaningless rate. A sample cut
/// off by the window is kept however small: that is a slow line, not a bad host.
fn sample_is_usable(bytes: u64, complete: bool) -> bool {
    !complete || bytes >= MIN_SAMPLE_BYTES
}

fn failed_probe(error: String) -> Probe {
    Probe {
        target: host_of(PAGE_URL),
        ok: false,
        latency_ms: None,
        speed_kbps: None,
        detail: error,
    }
}

fn throughput_kbps(bytes: u64, elapsed: Duration) -> Option<u64> {
    let seconds = elapsed.as_secs_f64();
    (seconds > 0.0).then(|| (bytes as f64 / seconds / 1024.0) as u64)
}

fn format_bytes(bytes: u64) -> String {
    let mib = bytes as f64 / 1024.0 / 1024.0;
    if mib >= 1.0 {
        format!("{mib:.1} MB")
    } else {
        format!("{} KB", bytes / 1024)
    }
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_else(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_is_bytes_over_the_measured_window() {
        assert_eq!(
            throughput_kbps(4 * 1024 * 1024, Duration::from_secs(4)),
            Some(1024)
        );
        assert_eq!(throughput_kbps(4 * 1024 * 1024, Duration::ZERO), None);
    }

    #[test]
    fn placeholder_responses_are_not_accepted_as_samples() {
        assert!(!sample_is_usable(24, true));
        assert!(sample_is_usable(4 * 1024 * 1024, true));
        assert!(sample_is_usable(12 * 1024, false));
    }

    #[test]
    fn probe_targets_are_hosts_not_urls() {
        assert_eq!(host_of(PAGE_URL), "www.gstatic.com");
        assert_eq!(host_of(SAMPLE_URLS[0]), "speed.cloudflare.com");
    }

    #[test]
    fn sample_sizes_read_as_pages_not_raw_bytes() {
        assert_eq!(format_bytes(4 * 1024 * 1024 + 512 * 1024), "4.5 MB");
        assert_eq!(format_bytes(200 * 1024), "200 KB");
    }
}
