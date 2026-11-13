use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

use super::progress::Reporter;
use super::quality::QualityMonitor;
use super::runtime::RuntimeManager;
use super::tun::TunManager;
use super::{config, integrations, routing, system_proxy};

#[derive(Default)]
pub struct AppState {
    pub runtime: RuntimeManager,
    pub tun: TunManager,
    /// Keeps probing the node list in the background, independent of whether a
    /// tunnel is up.
    pub quality: QualityMonitor,
    recovery_complete: AtomicBool,
    recovery_notify: Notify,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn recover_owned_network_state(&self) {
        if routing::has_stale_state() {
            let _ = routing::recover_stale_state().await;
        }
        if system_proxy::has_snapshot() {
            let _ = system_proxy::restore();
        }
        self.recovery_complete.store(true, Ordering::Release);
        self.recovery_notify.notify_waiters();
    }

    pub async fn wait_for_recovery(&self) {
        loop {
            if self.recovery_complete.load(Ordering::Acquire) {
                return;
            }
            let notified = self.recovery_notify.notified();
            if self.recovery_complete.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    pub async fn shutdown(&self) -> Result<(), String> {
        self.shutdown_reporting(&Reporter::silent()).await
    }

    /// Teardown always runs every step so that partially owned network state is
    /// released even when an earlier step fails; errors surface afterwards in
    /// the original order.
    pub async fn shutdown_reporting(&self, progress: &Reporter) -> Result<(), String> {
        self.wait_for_recovery().await;
        progress.stage("tun", "关闭 TUN 网卡");
        let tun_result = self.tun.stop().await;
        progress.stage("system-proxy", "还原系统代理");
        let proxy_result = system_proxy::restore();
        progress.stage("core", "停止本地代理内核");
        let runtime_result = self.runtime.stop().await;
        progress.stage("integrations", "还原 Git / SSH 代理");
        let integration_result = config::read_config().and_then(|current| {
            if current.git_proxy {
                integrations::set_git_proxy(false, super::runtime::DEFAULT_HTTP_PORT)?;
            }
            if current.ssh_proxy {
                integrations::set_ssh_proxy(false, super::runtime::DEFAULT_SOCKS_PORT)?;
            }
            Ok(())
        });
        tun_result?;
        proxy_result?;
        runtime_result?;
        integration_result?;
        progress.done("closed", "连接已断开");
        Ok(())
    }
}
