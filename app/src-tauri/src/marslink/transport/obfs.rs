//! MarsLink's thin wrapper around a classic Shadowsocks TCP stream.

use rand::RngExt;

pub const SESSION_PREFIX_LEN: usize = 60;
pub const HTTP_RANDOM_LEN: usize = 128;
pub const HEADER_END: &[u8; 4] = b"\r\n\r\n";

fn random_ay(length: usize) -> Vec<u8> {
    let mut rng = rand::rng();
    (0..length).map(|_| rng.random_range(b'a'..=b'y')).collect()
}

/// `token ":" random` padded to exactly 60 bytes for the normal 32-byte token.
/// The vendor client complements byte 1 before putting it on the wire.
pub fn session_prefix(token: &str) -> Result<Vec<u8>, String> {
    let mut prefix = token.as_bytes().to_vec();
    prefix.push(b':');
    if prefix.len() < SESSION_PREFIX_LEN {
        prefix.extend_from_slice(&random_ay(SESSION_PREFIX_LEN - prefix.len()));
    }
    if prefix.len() < 2 {
        return Err(String::from("token is too short for MarsLink prefix"));
    }
    prefix[1] ^= 0xff;
    Ok(prefix)
}

pub fn request_header(domain: &str, path: &str) -> Vec<u8> {
    let random =
        String::from_utf8(random_ay(HTTP_RANDOM_LEN)).expect("a-y random bytes are always UTF-8");
    format!(
        "POST {path}={random} HTTP/1.1\r\n\
         Host: {domain}\r\n\
         Connection: keep-alive\r\n\
         Pragma: no-cache\r\n\
         Cache-Control: no-cache\r\n\
         Transfer-Encoding: chunked\r\n\
         Content-Type: application/octet-stream\r\n\
         Upgrade-Insecure-Requests: 1\r\n\
         User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
         AppleWebKit/537.36 (KHTML, like Gecko) Chrome/108.0.0.0 Safari/537.36\r\n\
         Accept: text/html,application/xhtml+xml,application/xml;q=0.9,\
         image/avif,image/webp,image/apng,*/*;q=0.8,\
         application/signed-exchange;v=b3;q=0.9\r\n\
         Accept-Encoding: gzip, deflate\r\n\
         Accept-Language: zh-CN,zh;q=0.9,en-US;q=0.8,en;q=0.7,hy;q=0.6\r\n\
         \r\n"
    )
    .into_bytes()
}

pub fn find_header_end(data: &[u8]) -> Option<usize> {
    data.windows(HEADER_END.len())
        .position(|window| window == HEADER_END)
        .map(|position| position + HEADER_END.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_has_exact_shape() {
        let token = "0123456789abcdef0123456789abcdef";
        let prefix = session_prefix(token).unwrap();
        assert_eq!(prefix.len(), SESSION_PREFIX_LEN);
        assert_eq!(prefix[0], b'0');
        assert_eq!(prefix[1], b'1' ^ 0xff);
        assert_eq!(prefix[32], b':');
        assert!(prefix[33..].iter().all(|byte| (b'a'..=b'y').contains(byte)));
    }

    #[test]
    fn header_matches_vendor_shape() {
        let header = request_header("106.75.131.67:24812", "/sync?data=");
        let text = String::from_utf8(header).unwrap();
        assert!(text.starts_with("POST /sync?data=="));
        assert!(text.contains("\r\nHost: 106.75.131.67:24812\r\n"));
        assert!(text.ends_with("\r\n\r\n"));
        assert_eq!(find_header_end(text.as_bytes()), Some(text.len()));
    }
}
