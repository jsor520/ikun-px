pub mod http;
pub mod socks5;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::api::Node;
use super::fake_dns::FakeDns;
use super::rules::{RouteDecision, RuleEngine};
use super::transport::{Destination, MarsStream};

#[derive(Default)]
pub struct TrafficStats {
    upload: AtomicU64,
    download: AtomicU64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DomainEvent {
    pub timestamp: u64,
    pub host: String,
    pub route: RouteDecision,
}

#[derive(Default)]
pub struct DomainLog {
    entries: Mutex<VecDeque<DomainEvent>>,
}

impl DomainLog {
    fn push(&self, host: String, route: RouteDecision) {
        let mut entries = self.entries.lock().expect("domain log lock poisoned");
        if entries.len() >= 1000 {
            entries.pop_front();
        }
        entries.push_back(DomainEvent {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            host,
            route,
        });
    }

    pub fn list(&self) -> Vec<DomainEvent> {
        self.entries
            .lock()
            .expect("domain log lock poisoned")
            .iter()
            .rev()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.entries
            .lock()
            .expect("domain log lock poisoned")
            .clear();
    }
}

impl TrafficStats {
    pub fn upload(&self) -> u64 {
        self.upload.load(Ordering::Relaxed)
    }

    pub fn download(&self) -> u64 {
        self.download.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub struct ProxyContext {
    pub node: Node,
    pub token: String,
    pub traffic: Arc<TrafficStats>,
    pub rules: Arc<RwLock<RuleEngine>>,
    pub fake_dns: Arc<FakeDns>,
    pub domain_log: Arc<DomainLog>,
}

impl ProxyContext {
    pub fn new(
        node: Node,
        token: String,
        traffic: Arc<TrafficStats>,
        rules: Arc<RwLock<RuleEngine>>,
        fake_dns: Arc<FakeDns>,
        domain_log: Arc<DomainLog>,
    ) -> Self {
        Self {
            node,
            token,
            traffic,
            rules,
            fake_dns,
            domain_log,
        }
    }

    fn restore_fake_domain(&self, destination: Destination) -> Destination {
        match destination {
            Destination::Ip(std::net::IpAddr::V4(address), port) => self
                .fake_dns
                .domain_for(address)
                .map(|domain| Destination::Domain(domain, port))
                .unwrap_or(Destination::Ip(std::net::IpAddr::V4(address), port)),
            other => other,
        }
    }

    pub fn route_decision(&self, host: &str) -> RouteDecision {
        self.rules
            .read()
            .expect("routing rule lock poisoned")
            .decide(host)
    }

    pub async fn connect(&self, destination: Destination) -> Result<Upstream, String> {
        let destination = self.restore_fake_domain(destination);
        let host = destination.host();
        let decision = self.route_decision(&host);
        self.domain_log.push(host, decision);
        match decision {
            RouteDecision::Proxy => MarsStream::connect(&self.node, &self.token, destination)
                .await
                .map(Upstream::Mars),
            RouteDecision::Direct => {
                let endpoint = match destination {
                    Destination::Domain(host, port) => std::net::SocketAddr::new(
                        std::net::IpAddr::V4(self.fake_dns.resolve_direct(&host).await?),
                        port,
                    ),
                    Destination::Ip(address, port) => std::net::SocketAddr::new(address, port),
                };
                TcpStream::connect(endpoint)
                    .await
                    .map(Upstream::Direct)
                    .map_err(|e| format!("direct connection failed: {e}"))
            }
        }
    }
}

pub enum Upstream {
    Mars(MarsStream),
    Direct(TcpStream),
}

pub async fn relay(
    local: TcpStream,
    upstream: Upstream,
    initial_upload: &[u8],
    traffic: Arc<TrafficStats>,
) -> Result<(), String> {
    match upstream {
        Upstream::Mars(mars) => relay_mars(local, mars, initial_upload, traffic).await,
        Upstream::Direct(direct) => relay_direct(local, direct, initial_upload, traffic).await,
    }
}

async fn relay_direct(
    mut local: TcpStream,
    mut direct: TcpStream,
    initial_upload: &[u8],
    traffic: Arc<TrafficStats>,
) -> Result<(), String> {
    if !initial_upload.is_empty() {
        direct
            .write_all(initial_upload)
            .await
            .map_err(|e| format!("write direct initial payload: {e}"))?;
        traffic
            .upload
            .fetch_add(initial_upload.len() as u64, Ordering::Relaxed);
    }
    let (upload, download) = tokio::io::copy_bidirectional(&mut local, &mut direct)
        .await
        .map_err(|e| format!("relay direct connection: {e}"))?;
    traffic.upload.fetch_add(upload, Ordering::Relaxed);
    traffic.download.fetch_add(download, Ordering::Relaxed);
    Ok(())
}

async fn relay_mars(
    local: TcpStream,
    mars: MarsStream,
    initial_upload: &[u8],
    traffic: Arc<TrafficStats>,
) -> Result<(), String> {
    let (mut local_read, mut local_write) = local.into_split();
    let (mut mars_read, mut mars_write) = mars.into_split();

    if !initial_upload.is_empty() {
        mars_write.write_all(initial_upload).await?;
        traffic
            .upload
            .fetch_add(initial_upload.len() as u64, Ordering::Relaxed);
    }

    let upstream_traffic = Arc::clone(&traffic);
    let upstream = async move {
        let mut buffer = vec![0u8; 32 * 1024];
        loop {
            let count = local_read
                .read(&mut buffer)
                .await
                .map_err(|e| format!("read local client: {e}"))?;
            if count == 0 {
                let _ = mars_write.shutdown().await;
                return Ok(());
            }
            mars_write.write_all(&buffer[..count]).await?;
            upstream_traffic
                .upload
                .fetch_add(count as u64, Ordering::Relaxed);
        }
    };

    let downstream = async move {
        loop {
            match mars_read.read_chunk().await {
                Ok(chunk) => {
                    local_write
                        .write_all(&chunk)
                        .await
                        .map_err(|e| format!("write local client: {e}"))?;
                    traffic
                        .download
                        .fetch_add(chunk.len() as u64, Ordering::Relaxed);
                }
                Err(error) if error.contains("closed the connection") => {
                    let _ = local_write.shutdown().await;
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
        }
    };

    tokio::try_join!(upstream, downstream).map(|_| ())
}
