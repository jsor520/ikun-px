//! Rolling reachability stats for the node list.
//!
//! Mirrors what `PingMgr` does in the vendor client, read out of its
//! disassembly: a background thread ICMP-pings every node address, keeps the
//! last ten results per address in a queue, and reports `successes/queue size`
//! — which is why the denominator climbs to ten while the client runs instead
//! of being a fixed burst. The constants below are the vendor's: an 8 byte echo
//! payload with a 1000 ms timeout, one round every five seconds.

use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::task::JoinHandle;
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY,
};

use super::api::Node;

/// How many probes the ratio looks back over once the app has been running for
/// a while; before that the denominator is however many rounds have happened.
const WINDOW: usize = 10;
/// Measured from the start of a round, so the cadence does not drift with how
/// long the round itself takes.
const ROUND_PERIOD: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT_MS: u32 = 1000;
const PROBE_PAYLOAD: [u8; 8] = *b"ikun-px\0";

type History = HashMap<String, VecDeque<Option<u64>>>;

#[derive(Debug, Clone, Serialize)]
pub struct NodeMeasurement {
    pub uuid: String,
    /// Average of the successful probes still inside the window.
    pub latency_ms: Option<u64>,
    pub successful: u8,
    pub attempts: u8,
}

#[derive(Default)]
struct Tracking {
    uuids: Vec<String>,
    task: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct QualityMonitor {
    history: Arc<Mutex<History>>,
    tracking: tokio::sync::Mutex<Tracking>,
}

impl QualityMonitor {
    /// Probe `nodes` from now on. Returns immediately: the first round lands a
    /// moment later and the UI polls for it, so refreshing the node list is not
    /// held up by the network.
    pub async fn track(&self, nodes: Vec<Node>) {
        let uuids: Vec<String> = nodes.iter().map(|node| node.uuid.clone()).collect();
        let mut tracking = self.tracking.lock().await;
        let unchanged = tracking.uuids == uuids
            && tracking
                .task
                .as_ref()
                .is_some_and(|task| !task.is_finished());
        if unchanged {
            return;
        }

        if let Some(task) = tracking.task.take() {
            task.abort();
        }
        clear(&self.history);

        let history = Arc::clone(&self.history);
        tracking.task = Some(tokio::spawn(async move {
            loop {
                let started = Instant::now();
                probe_round(&nodes, &history).await;
                tokio::time::sleep(ROUND_PERIOD.saturating_sub(started.elapsed())).await;
            }
        }));
        tracking.uuids = uuids;
    }

    pub fn snapshot(&self) -> Vec<NodeMeasurement> {
        self.history
            .lock()
            .expect("node quality lock poisoned")
            .iter()
            .map(|(uuid, probes)| measurement(uuid, probes))
            .collect()
    }

    pub async fn stop(&self) {
        let mut tracking = self.tracking.lock().await;
        if let Some(task) = tracking.task.take() {
            task.abort();
        }
        tracking.uuids.clear();
        clear(&self.history);
    }
}

/// Sequential like the vendor's loop: several nodes usually share one host, and
/// firing the whole list at once gets the burst rate-limited and read back as
/// packet loss.
async fn probe_round(nodes: &[Node], history: &Arc<Mutex<History>>) {
    for node in nodes {
        let latency = probe(&node.addr).await;
        record(history, node.uuid.clone(), latency);
    }
}

async fn probe(addr: &str) -> Option<u64> {
    let target = resolve(addr).await?;
    tokio::task::spawn_blocking(move || echo(target))
        .await
        .ok()
        .flatten()
}

async fn resolve(addr: &str) -> Option<Ipv4Addr> {
    if let Ok(parsed) = addr.parse::<Ipv4Addr>() {
        return Some(parsed);
    }
    tokio::net::lookup_host((addr, 0))
        .await
        .ok()?
        .find_map(|resolved| match resolved.ip() {
            std::net::IpAddr::V4(address) => Some(address),
            std::net::IpAddr::V6(_) => None,
        })
}

/// The API writes the fixed header followed by the echoed payload, and asks for
/// eight spare bytes on top for the ICMP error case. Declaring it as a struct
/// rather than a byte array keeps the header aligned for reading back.
#[repr(C)]
struct EchoReply {
    header: ICMP_ECHO_REPLY,
    payload: [u8; PROBE_PAYLOAD.len() + 8],
}

/// One ICMP echo, returning the round trip the stack reports. Blocking, so
/// callers hand it to a blocking worker.
fn echo(target: Ipv4Addr) -> Option<u64> {
    let mut reply = EchoReply {
        header: unsafe { std::mem::zeroed() },
        payload: [0; PROBE_PAYLOAD.len() + 8],
    };
    unsafe {
        let handle: HANDLE = IcmpCreateFile();
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let replies = IcmpSendEcho(
            handle,
            u32::from_ne_bytes(target.octets()),
            PROBE_PAYLOAD.as_ptr() as *const c_void,
            PROBE_PAYLOAD.len() as u16,
            std::ptr::null(),
            &mut reply as *mut EchoReply as *mut c_void,
            size_of::<EchoReply>() as u32,
            PROBE_TIMEOUT_MS,
        );
        IcmpCloseHandle(handle);
        if replies == 0 {
            return None;
        }
    }
    (reply.header.Status == 0).then(|| u64::from(reply.header.RoundTripTime))
}

fn record(history: &Mutex<History>, uuid: String, latency: Option<u64>) {
    let mut history = history.lock().expect("node quality lock poisoned");
    let probes = history.entry(uuid).or_default();
    if probes.len() == WINDOW {
        probes.pop_front();
    }
    probes.push_back(latency);
}

fn clear(history: &Mutex<History>) {
    history
        .lock()
        .expect("node quality lock poisoned")
        .clear();
}

fn measurement(uuid: &str, probes: &VecDeque<Option<u64>>) -> NodeMeasurement {
    let successes: Vec<u64> = probes.iter().flatten().copied().collect();
    NodeMeasurement {
        uuid: uuid.to_string(),
        latency_ms: (!successes.is_empty())
            .then(|| successes.iter().sum::<u64>() / successes.len() as u64),
        successful: successes.len() as u8,
        attempts: probes.len() as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_of(probes: &[Option<u64>]) -> Mutex<History> {
        let history = Mutex::new(History::new());
        for probe in probes {
            record(&history, String::from("node"), *probe);
        }
        history
    }

    #[test]
    fn ratio_grows_with_the_rounds_until_the_window_is_full() {
        for rounds in [1usize, 5, WINDOW, WINDOW + 7] {
            let history = history_of(&vec![Some(30); rounds]);
            let history = history.lock().unwrap();
            let measured = measurement("node", &history["node"]);
            assert_eq!(measured.attempts as usize, rounds.min(WINDOW));
            assert_eq!(measured.successful, measured.attempts);
        }
    }

    #[test]
    fn latency_averages_only_the_probes_that_answered() {
        let history = history_of(&[Some(20), None, Some(40)]);
        let history = history.lock().unwrap();
        let measured = measurement("node", &history["node"]);
        assert_eq!(measured.latency_ms, Some(30));
        assert_eq!((measured.successful, measured.attempts), (2, 3));
    }

    #[test]
    fn a_node_that_never_answers_reports_no_latency() {
        let history = history_of(&[None, None]);
        let history = history.lock().unwrap();
        let measured = measurement("node", &history["node"]);
        assert_eq!(measured.latency_ms, None);
        assert_eq!((measured.successful, measured.attempts), (0, 2));
    }
}
