mod aead;
mod obfs;

use std::net::IpAddr;
use std::time::Duration;

use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::api::Node;
use aead::{CipherState, LENGTH_FRAME_LEN, MAX_CHUNK, SALT_LEN, TAG_LEN};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_HTTP_HEADER: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    Domain(String, u16),
    Ip(IpAddr, u16),
}

impl Destination {
    pub fn domain(host: impl Into<String>, port: u16) -> Self {
        Self::Domain(host.into(), port)
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut output = Vec::new();
        match self {
            Self::Domain(host, port) => {
                let host = host.as_bytes();
                if host.is_empty() || host.len() > u8::MAX as usize {
                    return Err(format!("invalid domain length {}", host.len()));
                }
                output.push(0x03);
                output.push(host.len() as u8);
                output.extend_from_slice(host);
                output.extend_from_slice(&port.to_be_bytes());
            }
            Self::Ip(IpAddr::V4(address), port) => {
                output.push(0x01);
                output.extend_from_slice(&address.octets());
                output.extend_from_slice(&port.to_be_bytes());
            }
            Self::Ip(IpAddr::V6(address), port) => {
                output.push(0x04);
                output.extend_from_slice(&address.octets());
                output.extend_from_slice(&port.to_be_bytes());
            }
        }
        Ok(output)
    }

    pub fn from_host(host: &str, port: u16) -> Self {
        host.parse::<IpAddr>()
            .map(|ip| Self::Ip(ip, port))
            .unwrap_or_else(|_| Self::Domain(host.to_string(), port))
    }

    pub fn host(&self) -> String {
        match self {
            Self::Domain(host, _) => host.clone(),
            Self::Ip(address, _) => address.to_string(),
        }
    }

    pub fn port(&self) -> u16 {
        match self {
            Self::Domain(_, port) | Self::Ip(_, port) => *port,
        }
    }
}

pub struct MarsStream {
    reader: MarsReader,
    writer: MarsWriter,
}

pub struct MarsReader {
    socket: OwnedReadHalf,
    decryptor: Option<CipherState>,
    incoming: BytesMut,
}

pub struct MarsWriter {
    socket: OwnedWriteHalf,
    encryptor: CipherState,
}

impl MarsStream {
    pub async fn connect(
        node: &Node,
        token: &str,
        destination: Destination,
    ) -> Result<Self, String> {
        let endpoint = format!("{}:{}", node.addr, node.port);
        let mut socket = timeout(CONNECT_TIMEOUT, TcpStream::connect(&endpoint))
            .await
            .map_err(|_| format!("timed out connecting to node {endpoint}"))?
            .map_err(|e| format!("connect to node {endpoint}: {e}"))?;
        socket
            .set_nodelay(true)
            .map_err(|e| format!("set TCP_NODELAY: {e}"))?;

        let salt = aead::random_salt();
        let mut encryptor = CipherState::new(&salt)?;
        let encrypted_target = encryptor.seal_chunk(&destination.encode()?)?;

        let domain = if node.happy_wife.is_empty() {
            endpoint.as_str()
        } else {
            node.happy_wife.as_str()
        };
        let path = if node.happy_life.is_empty() {
            "/building?arg"
        } else {
            node.happy_life.as_str()
        };

        let mut first_packet = obfs::request_header(domain, path);
        first_packet.extend_from_slice(&obfs::session_prefix(token)?);
        let mut wire_salt = salt;
        wire_salt[0] ^= 0xff;
        first_packet.extend_from_slice(&wire_salt);
        first_packet.extend_from_slice(&encrypted_target);

        timeout(IO_TIMEOUT, socket.write_all(&first_packet))
            .await
            .map_err(|_| String::from("timed out writing MarsLink first packet"))?
            .map_err(|e| format!("write MarsLink first packet: {e}"))?;

        let (read_half, write_half) = socket.into_split();
        Ok(Self {
            reader: MarsReader {
                socket: read_half,
                decryptor: None,
                incoming: BytesMut::with_capacity(16 * 1024),
            },
            writer: MarsWriter {
                socket: write_half,
                encryptor,
            },
        })
    }

    pub async fn write_all(&mut self, plaintext: &[u8]) -> Result<(), String> {
        self.writer.write_all(plaintext).await
    }

    pub async fn read_chunk(&mut self) -> Result<Vec<u8>, String> {
        self.reader.read_chunk().await
    }

    pub async fn shutdown(&mut self) -> Result<(), String> {
        self.writer.shutdown().await
    }

    pub fn into_split(self) -> (MarsReader, MarsWriter) {
        (self.reader, self.writer)
    }
}

impl MarsWriter {
    pub async fn write_all(&mut self, plaintext: &[u8]) -> Result<(), String> {
        for chunk in plaintext.chunks(MAX_CHUNK) {
            let encrypted = self.encryptor.seal_chunk(chunk)?;
            timeout(IO_TIMEOUT, self.socket.write_all(&encrypted))
                .await
                .map_err(|_| String::from("timed out writing encrypted payload"))?
                .map_err(|e| format!("write encrypted payload: {e}"))?;
        }
        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), String> {
        self.socket
            .shutdown()
            .await
            .map_err(|e| format!("shutdown node stream: {e}"))
    }
}

impl MarsReader {
    async fn fill(&mut self, needed: usize) -> Result<(), String> {
        while self.incoming.len() < needed {
            let read = timeout(IO_TIMEOUT, self.socket.read_buf(&mut self.incoming))
                .await
                .map_err(|_| String::from("timed out reading node response"))?
                .map_err(|e| format!("read node response: {e}"))?;
            if read == 0 {
                return Err(String::from("node closed the connection"));
            }
        }
        Ok(())
    }

    async fn initialize_decryptor(&mut self) -> Result<(), String> {
        if self.decryptor.is_some() {
            return Ok(());
        }

        self.fill(5).await?;
        if self.incoming.starts_with(b"HTTP/") {
            loop {
                if let Some(end) = obfs::find_header_end(&self.incoming) {
                    let _ = self.incoming.split_to(end);
                    break;
                }
                if self.incoming.len() >= MAX_HTTP_HEADER {
                    return Err(String::from("node HTTP camouflage header is too large"));
                }
                let before = self.incoming.len();
                self.fill(before + 1).await?;
            }
        }

        self.fill(SALT_LEN).await?;
        let salt_bytes = self.incoming.split_to(SALT_LEN);
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&salt_bytes);
        self.decryptor = Some(CipherState::new(&salt)?);
        Ok(())
    }

    pub async fn read_chunk(&mut self) -> Result<Vec<u8>, String> {
        self.initialize_decryptor().await?;
        self.fill(LENGTH_FRAME_LEN).await?;
        let length_frame = self.incoming.split_to(LENGTH_FRAME_LEN);
        let length = self
            .decryptor
            .as_mut()
            .expect("decryptor initialized")
            .open_length(&length_frame)?;

        self.fill(length + TAG_LEN).await?;
        let payload_frame = self.incoming.split_to(length + TAG_LEN);
        self.decryptor
            .as_mut()
            .expect("decryptor initialized")
            .open_payload(&payload_frame, length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_all_shadowsocks_address_types() {
        assert_eq!(
            Destination::domain("example.com", 443).encode().unwrap(),
            [&[3, 11][..], b"example.com", &443u16.to_be_bytes(),].concat()
        );
        assert_eq!(
            Destination::Ip(IpAddr::V4(std::net::Ipv4Addr::new(1, 2, 3, 4)), 53)
                .encode()
                .unwrap(),
            [1, 1, 2, 3, 4, 0, 53]
        );
        assert_eq!(
            Destination::Ip(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST), 80)
                .encode()
                .unwrap()
                .len(),
            19
        );
    }
}
