use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::api::Node;
use super::config::RouteMode;
use super::fake_dns::FakeDns;
use super::proxy::{http, socks5, DomainEvent, DomainLog, ProxyContext, TrafficStats};
use super::rules::RuleEngine;

pub const DEFAULT_HTTP_PORT: u16 = 11888;
pub const DEFAULT_SOCKS_PORT: u16 = 11889;

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    pub running: bool,
    pub node_uuid: String,
    pub node_name: String,
    pub http_address: String,
    pub socks5_address: String,
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

impl RuntimeStatus {
    fn stopped() -> Self {
        Self {
            running: false,
            node_uuid: String::new(),
            node_name: String::new(),
            http_address: String::new(),
            socks5_address: String::new(),
            upload_bytes: 0,
            download_bytes: 0,
        }
    }
}

struct Running {
    shutdown: watch::Sender<bool>,
    http_task: JoinHandle<Result<(), String>>,
    socks_task: JoinHandle<Result<(), String>>,
    node: Node,
    http_address: SocketAddr,
    socks_address: SocketAddr,
    traffic: Arc<TrafficStats>,
    domain_log: Arc<DomainLog>,
    rules: Arc<RwLock<RuleEngine>>,
}

#[derive(Default)]
pub struct RuntimeManager {
    running: Mutex<Option<Running>>,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start(
        &self,
        node: Node,
        token: String,
        http_port: u16,
        socks_port: u16,
    ) -> Result<RuntimeStatus, String> {
        self.start_with_rules(
            node,
            token,
            http_port,
            socks_port,
            Arc::new(RuleEngine::default()),
            Arc::new(FakeDns::default()),
        )
        .await
    }

    pub async fn start_with_rules(
        &self,
        node: Node,
        token: String,
        http_port: u16,
        socks_port: u16,
        rules: Arc<RuleEngine>,
        fake_dns: Arc<FakeDns>,
    ) -> Result<RuntimeStatus, String> {
        self.stop().await?;

        let bind_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let http_listener = TcpListener::bind(SocketAddr::new(bind_ip, http_port))
            .await
            .map_err(|e| format!("bind HTTP proxy 127.0.0.1:{http_port}: {e}"))?;
        let socks_listener = TcpListener::bind(SocketAddr::new(bind_ip, socks_port))
            .await
            .map_err(|e| format!("bind SOCKS5 proxy 127.0.0.1:{socks_port}: {e}"))?;
        let http_address = http_listener
            .local_addr()
            .map_err(|e| format!("read HTTP listener address: {e}"))?;
        let socks_address = socks_listener
            .local_addr()
            .map_err(|e| format!("read SOCKS5 listener address: {e}"))?;

        let traffic = Arc::new(TrafficStats::default());
        let domain_log = Arc::new(DomainLog::default());
        let rules = Arc::new(RwLock::new((*rules).clone()));
        let context = Arc::new(ProxyContext::new(
            node.clone(),
            token,
            Arc::clone(&traffic),
            Arc::clone(&rules),
            Arc::clone(&fake_dns),
            Arc::clone(&domain_log),
        ));
        let (shutdown, _) = watch::channel(false);
        let http_task = tokio::spawn(http::serve(
            http_listener,
            Arc::clone(&context),
            shutdown.subscribe(),
        ));
        let socks_task = tokio::spawn(socks5::serve(socks_listener, context, shutdown.subscribe()));

        let running = Running {
            shutdown,
            http_task,
            socks_task,
            node,
            http_address,
            socks_address,
            traffic,
            domain_log,
            rules,
        };
        let status = status_of(&running);
        *self.running.lock().await = Some(running);
        Ok(status)
    }

    pub async fn start_default(&self, node: Node, token: String) -> Result<RuntimeStatus, String> {
        self.start(node, token, DEFAULT_HTTP_PORT, DEFAULT_SOCKS_PORT)
            .await
    }

    pub async fn stop(&self) -> Result<(), String> {
        let Some(running) = self.running.lock().await.take() else {
            return Ok(());
        };
        let _ = running.shutdown.send(true);

        for task in [running.http_task, running.socks_task] {
            match timeout(Duration::from_secs(3), task).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(error))) => return Err(error),
                Ok(Err(error)) if error.is_cancelled() => {}
                Ok(Err(error)) => return Err(format!("proxy server task failed: {error}")),
                Err(_) => return Err(String::from("proxy server did not stop within 3 seconds")),
            }
        }
        Ok(())
    }

    pub async fn switch_node(&self, node: Node, token: String) -> Result<RuntimeStatus, String> {
        self.start_default(node, token).await
    }

    pub async fn status(&self) -> RuntimeStatus {
        self.running
            .lock()
            .await
            .as_ref()
            .map(status_of)
            .unwrap_or_else(RuntimeStatus::stopped)
    }

    /// The node the tunnel is actually built on, which can differ from the
    /// selected one while a switch is rolling back.
    pub async fn current_node(&self) -> Option<Node> {
        self.running
            .lock()
            .await
            .as_ref()
            .map(|running| running.node.clone())
    }

    pub async fn domain_events(&self) -> Vec<DomainEvent> {
        self.running
            .lock()
            .await
            .as_ref()
            .map(|running| running.domain_log.list())
            .unwrap_or_default()
    }

    pub async fn clear_domain_events(&self) {
        if let Some(running) = self.running.lock().await.as_ref() {
            running.domain_log.clear();
        }
    }

    pub async fn set_route_mode(&self, mode: RouteMode) -> Result<(), String> {
        let guard = self.running.lock().await;
        let Some(running) = guard.as_ref() else {
            return Ok(());
        };
        running
            .rules
            .write()
            .map_err(|_| String::from("routing rule lock poisoned"))?
            .set_mode(mode);
        Ok(())
    }

    pub async fn replace_rules(&self, rules: RuleEngine) -> Result<(), String> {
        let guard = self.running.lock().await;
        let Some(running) = guard.as_ref() else {
            return Ok(());
        };
        *running
            .rules
            .write()
            .map_err(|_| String::from("routing rule lock poisoned"))? = rules;
        Ok(())
    }
}

fn status_of(running: &Running) -> RuntimeStatus {
    RuntimeStatus {
        running: !running.http_task.is_finished() && !running.socks_task.is_finished(),
        node_uuid: running.node.uuid.clone(),
        node_name: running.node.name.clone(),
        http_address: running.http_address.to_string(),
        socks5_address: running.socks_address.to_string(),
        upload_bytes: running.traffic.upload(),
        download_bytes: running.traffic.download(),
    }
}
