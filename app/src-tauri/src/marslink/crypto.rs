//! MarsLink payload crypto.
//!
//! The server encrypts the node list and bootstrap config with a hardcoded
//! AES-256-CBC key and IV (both ASCII "1"s), then base64url-encodes the result.
//! Recovered by brute-forcing candidate keys from the binary against a captured
//! ciphertext (see re/crack.mjs).

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use aes::Aes256;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;

type Aes256CbcDec = cbc::Decryptor<Aes256>;

const KEY: &[u8; 32] = b"11111111111111111111111111111111";
const IV: &[u8; 16] = b"1111111111111111";

/// Decode base64url (tolerating missing padding) then AES-256-CBC decrypt.
pub fn decrypt_payload(b64url: &str) -> Result<Vec<u8>, String> {
    let trimmed = b64url.trim();
    let ct = URL_SAFE
        .decode(trimmed)
        .or_else(|_| URL_SAFE_NO_PAD.decode(trimmed))
        .map_err(|e| format!("base64url decode failed: {e}"))?;

    if ct.is_empty() || ct.len() % 16 != 0 {
        return Err(format!(
            "ciphertext length {} is not a multiple of 16",
            ct.len()
        ));
    }

    Aes256CbcDec::new(KEY.into(), IV.into())
        .decrypt_padded_vec_mut::<Pkcs7>(&ct)
        .map_err(|e| format!("aes-256-cbc decrypt failed: {e}"))
}

/// Decrypt a payload and parse it as UTF-8 JSON text.
pub fn decrypt_to_string(b64url: &str) -> Result<String, String> {
    let bytes = decrypt_payload(b64url)?;
    String::from_utf8(bytes).map_err(|e| format!("decrypted payload is not valid UTF-8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Bootstrap blob captured from ap.atyw.cn/greenday; decrypts to the api/web list.
    const BOOTSTRAP: &str = "p6qNpIuQW9SBkWN8RvZ5d1r_lNekki8p0ScwU5QTvJo3ORURFeWEDjZN_p-JEmVoHUNXsChVBPbWgDGJSKCg9Y9zpEZ9-hKldFWslZZKeJJ4I9jwEiYT6faFnMqWTiJONaBGtfHyfwtqCXgpVkDxmqSZUzxe00jL0v1XFrbR8Qh-KJt03eEhi19mAxdC0YR7al17hcOWdGJbNHiPNC1WBQ_tjhF_I3-WKAU4vMJVXCQ2KS8SngW5kbfCBnXw7w09CZbfnOsuOF2W-wCLJLbzsA==";

    #[test]
    fn decrypts_bootstrap() {
        let text = decrypt_to_string(BOOTSTRAP).expect("decrypt");
        assert!(text.contains("47.111.161.57"), "got: {text}");
        assert!(text.contains("\"api\""));
    }
}
