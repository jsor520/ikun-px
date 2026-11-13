use std::path::PathBuf;
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use super::routing;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DEVICE_URI: &str = "tun://ikun-px?GUID={73733B9D-DC9B-4AD4-95C7-EB78837E495F}";

struct RunningTun {
    child: Child,
}

#[derive(Default)]
pub struct TunManager {
    running: Mutex<Option<RunningTun>>,
}

impl TunManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start(&self, socks_port: u16) -> Result<(), String> {
        let mut guard = self.running.lock().await;
        if guard
            .as_mut()
            .is_some_and(|running| running.child.try_wait().ok().flatten().is_none())
        {
            return Ok(());
        }
        if routing::has_stale_state() {
            routing::cleanup().await?;
        }

        let executable = tun2socks_path()?;
        let mut child = Command::new(&executable)
            .current_dir(
                executable
                    .parent()
                    .ok_or_else(|| String::from("tun2socks has no parent directory"))?,
            )
            .args([
                "-device",
                DEVICE_URI,
                "-proxy",
                &format!("socks5://127.0.0.1:{socks_port}"),
                "-loglevel",
                "silent",
                "-tcp-auto-tuning",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("start tun2socks: {e}"))?;
        let pid = child
            .id()
            .ok_or_else(|| String::from("tun2socks did not expose a process ID"))?;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let interface_index = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| format!("inspect tun2socks: {e}"))?
            {
                return Err(format!("tun2socks exited during startup: {status}"));
            }
            if let Ok(index) = routing::adapter_index().await {
                break index;
            }
            if tokio::time::Instant::now() >= deadline {
                let _ = child.start_kill();
                return Err(String::from(
                    "timed out waiting for the ikun-px Wintun adapter",
                ));
            }
            sleep(Duration::from_millis(250)).await;
        };

        if let Err(error) = routing::configure(interface_index, pid).await {
            let _ = child.start_kill();
            let _ = timeout(Duration::from_secs(3), child.wait()).await;
            let _ = routing::cleanup().await;
            return Err(error);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("inspect tun2socks after configuration: {e}"))?
        {
            let _ = routing::cleanup().await;
            return Err(format!(
                "tun2socks exited after network configuration: {status}"
            ));
        }
        let confirmed_index = match routing::adapter_index().await {
            Ok(index) => index,
            Err(error) => {
                let _ = child.start_kill();
                let _ = timeout(Duration::from_secs(3), child.wait()).await;
                let _ = routing::cleanup().await;
                return Err(error);
            }
        };
        if confirmed_index != interface_index || !routing::has_stale_state() {
            let _ = child.start_kill();
            let _ = timeout(Duration::from_secs(3), child.wait()).await;
            let _ = routing::cleanup().await;
            return Err(String::from(
                "TUN adapter or owned route state disappeared during startup",
            ));
        }

        *guard = Some(RunningTun { child });
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let running = self.running.lock().await.take();
        if running.is_none() && !routing::has_stale_state() {
            return Ok(());
        }
        if let Some(mut running) = running {
            let _ = running.child.start_kill();
            let _ = timeout(Duration::from_secs(5), running.child.wait()).await;
            routing::cleanup().await
        } else {
            routing::recover_stale_state().await
        }
    }

    pub async fn is_running(&self) -> bool {
        self.running
            .lock()
            .await
            .as_mut()
            .is_some_and(|running| running.child.try_wait().ok().flatten().is_none())
    }
}

fn tun2socks_path() -> Result<PathBuf, String> {
    let installed = std::env::current_exe().ok().and_then(|exe| {
        exe.parent()
            .map(|dir| dir.join("tun").join("tun2socks.exe"))
    });
    if let Some(path) = installed.filter(|path| path.is_file()) {
        return Ok(path);
    }

    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("tun")
        .join("tun2socks.exe");
    development
        .is_file()
        .then_some(development)
        .ok_or_else(|| String::from("bundled tun2socks.exe was not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_sidecar_exists() {
        let path = tun2socks_path().unwrap();
        assert!(path.ends_with("tun2socks.exe"));
        assert!(path.is_file());
    }
}
