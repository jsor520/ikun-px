use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use tokio::time::timeout;

const FAKE_NET_FIRST: u32 = u32::from_be_bytes([44, 0, 0, 1]);
const FAKE_NET_LAST: u32 = u32::from_be_bytes([44, 255, 255, 254]);
const DIRECT_DNS_SERVERS: &[&str] = &["223.5.5.5:53", "119.29.29.29:53"];
const DIRECT_DNS_TIMEOUT: Duration = Duration::from_millis(1500);
const DIRECT_CACHE_TTL: Duration = Duration::from_secs(60);

struct CachedAddress {
    address: Ipv4Addr,
    expires_at: Instant,
}

#[derive(Default)]
struct State {
    next: u32,
    by_domain: HashMap<String, Ipv4Addr>,
    by_ip: HashMap<Ipv4Addr, String>,
    direct_cache: HashMap<String, CachedAddress>,
}

pub struct FakeDns {
    state: Mutex<State>,
}

impl Default for FakeDns {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                next: FAKE_NET_FIRST,
                ..State::default()
            }),
        }
    }
}

impl FakeDns {
    pub fn allocate(&self, domain: &str) -> Ipv4Addr {
        let domain = domain.trim_end_matches('.').to_ascii_lowercase();
        let mut state = self.state.lock().expect("fake DNS lock poisoned");
        if let Some(address) = state.by_domain.get(&domain) {
            return *address;
        }

        let address = Ipv4Addr::from(state.next);
        state.next = if state.next >= FAKE_NET_LAST {
            FAKE_NET_FIRST
        } else {
            state.next + 1
        };
        if let Some(previous_domain) = state.by_ip.insert(address, domain.clone()) {
            state.by_domain.remove(&previous_domain);
        }
        state.by_domain.insert(domain, address);
        address
    }

    pub fn domain_for(&self, address: Ipv4Addr) -> Option<String> {
        self.state
            .lock()
            .expect("fake DNS lock poisoned")
            .by_ip
            .get(&address)
            .cloned()
    }

    pub fn answer(&self, query: &[u8]) -> Result<Vec<u8>, String> {
        let parsed = parse_question(query)?;
        let answer_ip = (parsed.query_type == 1 && parsed.query_class == 1)
            .then(|| self.allocate(&parsed.domain));

        let mut response = query[..parsed.question_end].to_vec();
        response[2] = (query[2] & 0x79) | 0x80;
        response[3] = 0x80;
        response[4..6].copy_from_slice(&1u16.to_be_bytes());
        response[6..8].copy_from_slice(&(answer_ip.is_some() as u16).to_be_bytes());
        response[8..10].copy_from_slice(&0u16.to_be_bytes());
        response[10..12].copy_from_slice(&0u16.to_be_bytes());

        if let Some(address) = answer_ip {
            response.extend_from_slice(&[
                0xc0, 0x0c, // compressed name pointer
                0x00, 0x01, // A
                0x00, 0x01, // IN
                0x00, 0x00, 0x00, 0x3c, // 60-second TTL
                0x00, 0x04,
            ]);
            response.extend_from_slice(&address.octets());
        }
        Ok(response)
    }

    pub fn question_domain(&self, query: &[u8]) -> Result<String, String> {
        Ok(parse_question(query)?.domain)
    }

    /// Forward a direct-rule DNS request outside the fake-IP route.
    pub async fn forward_direct(&self, query: &[u8]) -> Result<Vec<u8>, String> {
        let query_id = query
            .get(..2)
            .ok_or_else(|| String::from("direct DNS query is too short"))?;
        let mut failures = Vec::new();
        for server in DIRECT_DNS_SERVERS {
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
                .await
                .map_err(|e| format!("bind direct DNS socket: {e}"))?;
            if let Err(error) = socket.send_to(query, server).await {
                failures.push(format!("{server}: send failed: {error}"));
                continue;
            }
            let mut response = vec![0u8; 4096];
            match timeout(DIRECT_DNS_TIMEOUT, socket.recv_from(&mut response)).await {
                Ok(Ok((count, source)))
                    if source.to_string() == *server
                        && response.get(..2).is_some_and(|id| id == query_id) =>
                {
                    response.truncate(count);
                    return Ok(response);
                }
                Ok(Ok(_)) => failures.push(format!("{server}: mismatched response")),
                Ok(Err(error)) => failures.push(format!("{server}: receive failed: {error}")),
                Err(_) => failures.push(format!("{server}: timed out")),
            }
        }
        Err(format!("direct DNS failed ({})", failures.join("; ")))
    }

    pub async fn resolve_direct(&self, domain: &str) -> Result<Ipv4Addr, String> {
        let domain = domain.trim_end_matches('.').to_ascii_lowercase();
        {
            let mut state = self.state.lock().expect("fake DNS lock poisoned");
            if let Some(cached) = state.direct_cache.get(&domain) {
                if cached.expires_at > Instant::now() {
                    return Ok(cached.address);
                }
            }
            state.direct_cache.remove(&domain);
        }

        let query = build_a_query(&domain);
        let response = self.forward_direct(&query).await?;
        let address = first_a_answer(&response)
            .ok_or_else(|| format!("direct DNS returned no A record for {domain}"))?;
        let now = Instant::now();
        let mut state = self.state.lock().expect("fake DNS lock poisoned");
        state
            .direct_cache
            .retain(|_, cached| cached.expires_at > now);
        if state.direct_cache.len() >= 1024 {
            state.direct_cache.clear();
        }
        state.direct_cache.insert(
            domain,
            CachedAddress {
                address,
                expires_at: now + DIRECT_CACHE_TTL,
            },
        );
        Ok(address)
    }
}

struct DnsQuestion {
    domain: String,
    query_type: u16,
    query_class: u16,
    question_end: usize,
}

fn parse_question(packet: &[u8]) -> Result<DnsQuestion, String> {
    if packet.len() < 17 {
        return Err(String::from("DNS packet is too short"));
    }
    if u16::from_be_bytes([packet[4], packet[5]]) == 0 {
        return Err(String::from("DNS packet has no questions"));
    }
    let mut offset = 12;
    let mut labels = Vec::new();
    loop {
        let length = *packet
            .get(offset)
            .ok_or_else(|| String::from("truncated DNS question name"))?
            as usize;
        offset += 1;
        if length == 0 {
            break;
        }
        if length > 63 || offset + length > packet.len() {
            return Err(String::from("invalid DNS label"));
        }
        labels.push(
            std::str::from_utf8(&packet[offset..offset + length])
                .map_err(|_| String::from("DNS label is not UTF-8"))?,
        );
        offset += length;
    }
    if labels.is_empty() || offset + 4 > packet.len() {
        return Err(String::from("invalid DNS question"));
    }
    let query_type = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
    let query_class = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
    Ok(DnsQuestion {
        domain: labels.join("."),
        query_type,
        query_class,
        question_end: offset + 4,
    })
}

fn build_a_query(domain: &str) -> Vec<u8> {
    let id: u16 = rand::random();
    let mut packet = Vec::with_capacity(64);
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&[0x01, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
    for label in domain.trim_end_matches('.').split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.extend_from_slice(&[0, 0, 1, 0, 1]);
    packet
}

fn skip_dns_name(packet: &[u8], offset: &mut usize) -> Option<()> {
    loop {
        let length = *packet.get(*offset)?;
        if length & 0xc0 == 0xc0 {
            *offset += 2;
            return (*offset <= packet.len()).then_some(());
        }
        *offset += 1;
        if length == 0 {
            return Some(());
        }
        *offset += length as usize;
        if *offset > packet.len() {
            return None;
        }
    }
}

fn first_a_answer(packet: &[u8]) -> Option<Ipv4Addr> {
    if packet.len() < 12 {
        return None;
    }
    let questions = u16::from_be_bytes([packet[4], packet[5]]) as usize;
    let answers = u16::from_be_bytes([packet[6], packet[7]]) as usize;
    let mut offset = 12;
    for _ in 0..questions {
        skip_dns_name(packet, &mut offset)?;
        offset = offset.checked_add(4)?;
        if offset > packet.len() {
            return None;
        }
    }
    for _ in 0..answers {
        skip_dns_name(packet, &mut offset)?;
        if offset + 10 > packet.len() {
            return None;
        }
        let record_type = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        let class = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
        let length = u16::from_be_bytes([packet[offset + 8], packet[offset + 9]]) as usize;
        offset += 10;
        if offset + length > packet.len() {
            return None;
        }
        if record_type == 1 && class == 1 && length == 4 {
            return Some(Ipv4Addr::new(
                packet[offset],
                packet[offset + 1],
                packet[offset + 2],
                packet[offset + 3],
            ));
        }
        offset += length;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_query() -> Vec<u8> {
        vec![
            0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, b'e',
            b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00,
            0x01,
        ]
    }

    #[test]
    fn returns_stable_fake_a_record_and_reverse_mapping() {
        let dns = FakeDns::default();
        let first = dns.answer(&a_query()).unwrap();
        let second = dns.answer(&a_query()).unwrap();
        assert_eq!(&first[first.len() - 4..], &[44, 0, 0, 1]);
        assert_eq!(&second[second.len() - 4..], &[44, 0, 0, 1]);
        assert_eq!(
            dns.domain_for(Ipv4Addr::new(44, 0, 0, 1)).as_deref(),
            Some("example.com")
        );
        assert_eq!(first_a_answer(&first), Some(Ipv4Addr::new(44, 0, 0, 1)));
    }

    #[tokio::test]
    async fn direct_cache_normalizes_domain_names() {
        let dns = FakeDns::default();
        let expected = Ipv4Addr::new(203, 0, 113, 7);
        dns.state.lock().unwrap().direct_cache.insert(
            "example.com".to_string(),
            CachedAddress {
                address: expected,
                expires_at: Instant::now() + Duration::from_secs(30),
            },
        );
        assert_eq!(dns.resolve_direct("Example.COM.").await.unwrap(), expected);
    }
}
