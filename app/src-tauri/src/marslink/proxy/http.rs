use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use url::Url;

use super::{relay, ProxyContext};
use crate::marslink::transport::Destination;

const MAX_HEADER: usize = 64 * 1024;

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
                let (stream, _) = accepted.map_err(|e| format!("HTTP proxy accept: {e}"))?;
                let context = Arc::clone(&context);
                connections.spawn(async move {
                    if let Err(error) = handle_client(stream, context).await {
                        eprintln!("HTTP proxy connection: {error}");
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
    let initial = read_initial_request(&mut local).await?;
    let parsed = match parse_request(&initial) {
        Ok(parsed) => parsed,
        Err(error) => {
            let _ = local
                .write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n")
                .await;
            return Err(error);
        }
    };

    let upstream = match context.connect(parsed.destination).await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = local
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n")
                .await;
            return Err(error);
        }
    };

    if parsed.is_connect {
        local
            .write_all(
                b"HTTP/1.1 200 Connection Established\r\n\
                  Proxy-Agent: IKUN-PX\r\n\
                  \r\n",
            )
            .await
            .map_err(|e| format!("write HTTP CONNECT response: {e}"))?;
    }

    relay(
        local,
        upstream,
        &parsed.upstream_initial,
        Arc::clone(&context.traffic),
    )
    .await
}

async fn read_initial_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    loop {
        let count = stream
            .read(&mut chunk)
            .await
            .map_err(|e| format!("read HTTP proxy request: {e}"))?;
        if count == 0 {
            return Err(String::from("HTTP proxy client closed before request"));
        }
        buffer.extend_from_slice(&chunk[..count]);
        if header_end(&buffer).is_some() {
            return Ok(buffer);
        }
        if buffer.len() >= MAX_HEADER {
            return Err(String::from("HTTP proxy request header is too large"));
        }
    }
}

struct ParsedRequest {
    destination: Destination,
    is_connect: bool,
    upstream_initial: Vec<u8>,
}

fn parse_request(input: &[u8]) -> Result<ParsedRequest, String> {
    let end = header_end(input).ok_or_else(|| String::from("incomplete HTTP request"))?;
    let header = std::str::from_utf8(&input[..end])
        .map_err(|_| String::from("HTTP request header is not UTF-8"))?;
    let mut lines = header.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| String::from("missing HTTP request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| String::from("missing HTTP method"))?;
    let target = parts
        .next()
        .ok_or_else(|| String::from("missing HTTP target"))?;
    let version = parts
        .next()
        .ok_or_else(|| String::from("missing HTTP version"))?;

    if method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = parse_authority(target, 443)?;
        return Ok(ParsedRequest {
            destination: Destination::from_host(&host, port),
            is_connect: true,
            upstream_initial: input[end..].to_vec(),
        });
    }

    let (destination, origin_target) =
        if target.starts_with("http://") || target.starts_with("https://") {
            let url = Url::parse(target).map_err(|e| format!("invalid proxy URL: {e}"))?;
            let host = url
                .host_str()
                .ok_or_else(|| String::from("proxy URL has no host"))?;
            let port = url
                .port_or_known_default()
                .ok_or_else(|| String::from("proxy URL has no port"))?;
            let mut origin = url.path().to_string();
            if origin.is_empty() {
                origin.push('/');
            }
            if let Some(query) = url.query() {
                origin.push('?');
                origin.push_str(query);
            }
            (Destination::from_host(host, port), origin)
        } else {
            let host_header = lines
                .clone()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("host")
                            .then(|| value.trim().to_string())
                    })
                })
                .ok_or_else(|| String::from("origin-form HTTP request has no Host header"))?;
            let (host, port) = parse_authority(&host_header, 80)?;
            (Destination::from_host(&host, port), target.to_string())
        };

    let mut rewritten = format!("{method} {origin_target} {version}\r\n").into_bytes();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if line
            .split_once(':')
            .is_some_and(|(name, _)| name.eq_ignore_ascii_case("proxy-connection"))
        {
            continue;
        }
        rewritten.extend_from_slice(line.as_bytes());
        rewritten.extend_from_slice(b"\r\n");
    }
    rewritten.extend_from_slice(b"\r\n");
    rewritten.extend_from_slice(&input[end..]);

    Ok(ParsedRequest {
        destination,
        is_connect: false,
        upstream_initial: rewritten,
    })
}

fn header_end(data: &[u8]) -> Option<usize> {
    data.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn parse_authority(authority: &str, default_port: u16) -> Result<(String, u16), String> {
    if let Some(rest) = authority.strip_prefix('[') {
        let close = rest
            .find(']')
            .ok_or_else(|| String::from("invalid bracketed IPv6 authority"))?;
        let host = rest[..close].to_string();
        let suffix = &rest[close + 1..];
        let port = if suffix.is_empty() {
            default_port
        } else {
            suffix
                .strip_prefix(':')
                .ok_or_else(|| String::from("invalid IPv6 authority suffix"))?
                .parse()
                .map_err(|_| String::from("invalid authority port"))?
        };
        return Ok((host, port));
    }
    if let Some((host, port)) = authority.rsplit_once(':') {
        if !host.contains(':') {
            if let Ok(port) = port.parse::<u16>() {
                return Ok((host.to_string(), port));
            }
        }
    }
    Ok((authority.to_string(), default_port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_connect_and_preserves_early_tls_bytes() {
        let parsed =
            parse_request(b"CONNECT example.com:443 HTTP/1.1\r\nHost: x\r\n\r\nabc").unwrap();
        assert!(parsed.is_connect);
        assert_eq!(parsed.destination, Destination::domain("example.com", 443));
        assert_eq!(parsed.upstream_initial, b"abc");
    }

    #[test]
    fn rewrites_absolute_http_uri() {
        let parsed = parse_request(
            b"GET http://example.com:8080/a?q=1 HTTP/1.1\r\n\
              Host: example.com:8080\r\n\
              Proxy-Connection: keep-alive\r\n\r\n",
        )
        .unwrap();
        assert!(!parsed.is_connect);
        assert_eq!(parsed.destination, Destination::domain("example.com", 8080));
        assert!(parsed
            .upstream_initial
            .starts_with(b"GET /a?q=1 HTTP/1.1\r\n"));
        assert!(!String::from_utf8_lossy(&parsed.upstream_initial)
            .to_ascii_lowercase()
            .contains("proxy-connection"));
    }
}
