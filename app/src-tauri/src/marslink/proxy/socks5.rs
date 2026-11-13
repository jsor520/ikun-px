use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::watch;
use tokio::task::JoinSet;

use super::{relay, ProxyContext};
use crate::marslink::transport::Destination;

const VERSION: u8 = 0x05;
const NO_AUTH: u8 = 0x00;
const NO_ACCEPTABLE_AUTH: u8 = 0xff;

pub async fn serve(
    listener: TcpListener,
    context: Arc<ProxyContext>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted.map_err(|e| format!("SOCKS5 accept: {e}"))?;
                let context = Arc::clone(&context);
                connections.spawn(async move {
                    if let Err(error) = handle_client(stream, context).await {
                        eprintln!("SOCKS5 connection: {error}");
                    }
                });
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    connections.shutdown().await;
    Ok(())
}

async fn handle_client(mut local: TcpStream, context: Arc<ProxyContext>) -> Result<(), String> {
    negotiate_auth(&mut local).await?;
    let request = match read_request(&mut local).await {
        Ok(request) => request,
        Err((reply_code, error)) => {
            let _ = send_reply(&mut local, reply_code).await;
            return Err(error);
        }
    };

    let SocksRequest::Connect(destination) = request else {
        return handle_udp_associate(local, context).await;
    };
    let upstream = match context.connect(destination).await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = send_reply(&mut local, 0x05).await;
            return Err(error);
        }
    };

    send_reply(&mut local, 0x00).await?;
    relay(local, upstream, &[], Arc::clone(&context.traffic)).await
}

async fn negotiate_auth(stream: &mut TcpStream) -> Result<(), String> {
    let mut greeting = [0u8; 2];
    stream
        .read_exact(&mut greeting)
        .await
        .map_err(|e| format!("read SOCKS5 greeting: {e}"))?;
    if greeting[0] != VERSION || greeting[1] == 0 {
        return Err(String::from("invalid SOCKS5 greeting"));
    }
    let mut methods = vec![0u8; greeting[1] as usize];
    stream
        .read_exact(&mut methods)
        .await
        .map_err(|e| format!("read SOCKS5 methods: {e}"))?;
    let selected = if methods.contains(&NO_AUTH) {
        NO_AUTH
    } else {
        NO_ACCEPTABLE_AUTH
    };
    stream
        .write_all(&[VERSION, selected])
        .await
        .map_err(|e| format!("write SOCKS5 auth selection: {e}"))?;
    if selected == NO_ACCEPTABLE_AUTH {
        return Err(String::from("SOCKS5 client does not offer no-auth"));
    }
    Ok(())
}

enum SocksRequest {
    Connect(Destination),
    UdpAssociate,
}

async fn read_request(stream: &mut TcpStream) -> Result<SocksRequest, (u8, String)> {
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|e| (0x01, format!("read SOCKS5 request: {e}")))?;
    if header[0] != VERSION || header[2] != 0 {
        return Err((0x01, String::from("invalid SOCKS5 request header")));
    }
    if header[1] != 0x01 && header[1] != 0x03 {
        return Err((0x07, format!("unsupported SOCKS5 command {}", header[1])));
    }

    let destination = match header[3] {
        0x01 => {
            let mut bytes = [0u8; 4];
            stream
                .read_exact(&mut bytes)
                .await
                .map_err(|e| (0x01, format!("read SOCKS5 IPv4: {e}")))?;
            Destination::Ip(IpAddr::V4(Ipv4Addr::from(bytes)), read_port(stream).await?)
        }
        0x03 => {
            let length = stream
                .read_u8()
                .await
                .map_err(|e| (0x01, format!("read SOCKS5 domain length: {e}")))?
                as usize;
            if length == 0 {
                return Err((0x08, String::from("empty SOCKS5 domain")));
            }
            let mut bytes = vec![0u8; length];
            stream
                .read_exact(&mut bytes)
                .await
                .map_err(|e| (0x01, format!("read SOCKS5 domain: {e}")))?;
            let host = String::from_utf8(bytes)
                .map_err(|_| (0x08, String::from("SOCKS5 domain is not UTF-8")))?;
            Destination::Domain(host, read_port(stream).await?)
        }
        0x04 => {
            let mut bytes = [0u8; 16];
            stream
                .read_exact(&mut bytes)
                .await
                .map_err(|e| (0x01, format!("read SOCKS5 IPv6: {e}")))?;
            Destination::Ip(IpAddr::V6(Ipv6Addr::from(bytes)), read_port(stream).await?)
        }
        atyp => return Err((0x08, format!("unsupported SOCKS5 address type {atyp}"))),
    };
    if header[1] == 0x03 {
        Ok(SocksRequest::UdpAssociate)
    } else {
        Ok(SocksRequest::Connect(destination))
    }
}

async fn read_port(stream: &mut TcpStream) -> Result<u16, (u8, String)> {
    stream
        .read_u16()
        .await
        .map_err(|e| (0x01, format!("read SOCKS5 port: {e}")))
}

async fn send_reply(stream: &mut TcpStream, code: u8) -> Result<(), String> {
    send_reply_from(
        stream,
        code,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
    )
    .await
}

async fn send_reply_from(
    stream: &mut TcpStream,
    code: u8,
    bound: SocketAddr,
) -> Result<(), String> {
    let mut response = vec![VERSION, code, 0x00, 0x01];
    if let IpAddr::V4(address) = bound.ip() {
        response.extend_from_slice(&address.octets());
    } else {
        response[3] = 0x04;
        if let IpAddr::V6(address) = bound.ip() {
            response.extend_from_slice(&address.octets());
        }
    }
    response.extend_from_slice(&bound.port().to_be_bytes());
    stream
        .write_all(&response)
        .await
        .map_err(|e| format!("write SOCKS5 reply: {e}"))
}

async fn handle_udp_associate(
    mut control: TcpStream,
    context: Arc<ProxyContext>,
) -> Result<(), String> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|e| format!("bind SOCKS5 UDP socket: {e}"))?;
    let bound = socket
        .local_addr()
        .map_err(|e| format!("read SOCKS5 UDP address: {e}"))?;
    send_reply_from(&mut control, 0x00, bound).await?;

    let mut datagram = vec![0u8; u16::MAX as usize];
    let mut control_byte = [0u8; 1];
    loop {
        tokio::select! {
            read = control.read(&mut control_byte) => {
                match read {
                    Ok(0) | Err(_) => return Ok(()),
                    Ok(_) => {}
                }
            }
            received = socket.recv_from(&mut datagram) => {
                let (length, sender) =
                    received.map_err(|e| format!("receive SOCKS5 UDP datagram: {e}"))?;
                let Some((header_length, destination)) =
                    parse_udp_request(&datagram[..length])?
                else {
                    continue;
                };
                if destination.port() != 53 {
                    continue;
                }
                let query = &datagram[header_length..];
                let direct = context
                    .fake_dns
                    .question_domain(query)
                    .ok()
                    .is_some_and(|domain| {
                        context.route_decision(&domain)
                            == crate::marslink::rules::RouteDecision::Direct
                    });
                let dns_response = match if direct {
                    context.fake_dns.forward_direct(query).await
                } else {
                    context.fake_dns.answer(query)
                } {
                    Ok(response) => response,
                    Err(_) => continue,
                };
                let mut response = encode_udp_header(&destination)?;
                response.extend_from_slice(&dns_response);
                socket
                    .send_to(&response, sender)
                    .await
                    .map_err(|e| format!("send SOCKS5 UDP response: {e}"))?;
            }
        }
    }
}

fn parse_udp_request(data: &[u8]) -> Result<Option<(usize, Destination)>, String> {
    if data.len() < 7 || data[0..2] != [0, 0] {
        return Ok(None);
    }
    if data[2] != 0 {
        return Ok(None);
    }
    let mut offset = 4;
    let destination = match data[3] {
        0x01 => {
            if data.len() < offset + 4 + 2 {
                return Ok(None);
            }
            let address = Ipv4Addr::new(
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            );
            offset += 4;
            let port = u16::from_be_bytes([data[offset], data[offset + 1]]);
            offset += 2;
            Destination::Ip(IpAddr::V4(address), port)
        }
        0x03 => {
            let Some(&length) = data.get(offset) else {
                return Ok(None);
            };
            offset += 1;
            let length = length as usize;
            if data.len() < offset + length + 2 {
                return Ok(None);
            }
            let host = std::str::from_utf8(&data[offset..offset + length])
                .map_err(|_| String::from("SOCKS5 UDP domain is not UTF-8"))?
                .to_string();
            offset += length;
            let port = u16::from_be_bytes([data[offset], data[offset + 1]]);
            offset += 2;
            Destination::Domain(host, port)
        }
        0x04 => {
            if data.len() < offset + 16 + 2 {
                return Ok(None);
            }
            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(&data[offset..offset + 16]);
            offset += 16;
            let port = u16::from_be_bytes([data[offset], data[offset + 1]]);
            offset += 2;
            Destination::Ip(IpAddr::V6(Ipv6Addr::from(bytes)), port)
        }
        _ => return Ok(None),
    };
    Ok(Some((offset, destination)))
}

fn encode_udp_header(destination: &Destination) -> Result<Vec<u8>, String> {
    let mut header = vec![0, 0, 0];
    header.extend_from_slice(&destination.encode()?);
    Ok(header)
}
