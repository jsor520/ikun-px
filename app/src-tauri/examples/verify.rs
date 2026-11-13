//! Standalone end-to-end check of the interop core, no UI and no elevation:
//! reads independent DPAPI-backed state, calls the live API, and decrypts the
//! node list. Use `proxy_probe` for live local-proxy verification.
//!
//! Run with:  cargo run --example verify

use std::time::Duration;

use ikun_px_lib::marslink::quality::QualityMonitor;
use ikun_px_lib::marslink::{self, api, config};

#[tokio::main]
async fn main() {
    println!("== ikun-px core verifier ==\n");

    println!("[1] reading independent IKUN PX config ...");
    let cfg = match config::read_config() {
        Ok(c) => {
            println!(
                "    email={} token={}... selected={} capture={:?} route={:?}",
                c.email,
                c.token.chars().take(6).collect::<String>(),
                c.last_select_node,
                c.capture_mode,
                c.route_mode
            );
            c
        }
        Err(e) => {
            eprintln!("    FAILED: {e}");
            return;
        }
    };

    println!("\n[2] building API context ...");
    let ctx = match marslink::build_context() {
        Ok(c) => {
            println!(
                "    api_base={} version={} deviceid={}",
                c.api_base, c.version, c.device_id
            );
            c
        }
        Err(e) => {
            eprintln!("    FAILED: {e}");
            return;
        }
    };

    println!("\n[3] GET /appuser/userinfo ...");
    match api::user_info(&ctx).await {
        Ok(u) => {
            let gib = u.transfer.parse::<f64>().unwrap_or(0.0) / 1024.0 / 1024.0 / 1024.0;
            println!(
                "    email={} transfer_left={:.2} GiB expire_ts={}",
                u.email, gib, u.charge_expire_date
            );
        }
        Err(e) => eprintln!("    FAILED: {e}"),
    }

    println!("\n[4] GET /system/clientnodelist (+decrypt) ...");
    let nodes = match api::node_list(&ctx).await {
        Ok(nodes) => {
            println!("    decrypted {} nodes:", nodes.len());
            for n in nodes.iter().take(30) {
                let mark = if n.uuid == cfg.last_select_node {
                    " <== selected"
                } else {
                    ""
                };
                println!(
                    "      {:<10} {:<26} {}:{}{}",
                    &n.uuid[..8.min(n.uuid.len())],
                    n.name,
                    n.addr,
                    n.port,
                    mark
                );
            }
            Some(nodes)
        }
        Err(e) => {
            eprintln!("    FAILED: {e}");
            None
        }
    };

    println!("\n[5] rolling node quality, the ratio should climb per round ...");
    if let Some(nodes) = nodes {
        let selected = cfg.last_select_node.clone();
        let monitor = QualityMonitor::default();
        monitor.track(nodes).await;
        for round in 1..=3 {
            tokio::time::sleep(Duration::from_secs(6)).await;
            let snapshot = monitor.snapshot();
            let tracked = snapshot
                .iter()
                .find(|measured| measured.uuid == selected)
                .or_else(|| snapshot.first());
            match tracked {
                Some(measured) => println!(
                    "    round {round}: {} nodes measured, sample {}/{} at {}",
                    snapshot.len(),
                    measured.successful,
                    measured.attempts,
                    measured
                        .latency_ms
                        .map(|ms| format!("{ms} ms"))
                        .unwrap_or_else(|| String::from("timeout"))
                ),
                None => println!("    round {round}: no measurements yet"),
            }
        }
        monitor.stop().await;
        if !monitor.snapshot().is_empty() {
            eprintln!("    FAILED: stopping the monitor left stale measurements");
        }
    }

    println!("\n== done ==");
}
