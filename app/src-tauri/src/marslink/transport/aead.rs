//! Classic Shadowsocks AEAD codec used by MarsLink.
//!
//! Wire format after the MarsLink-specific preface is the standard
//! `chacha20-ietf-poly1305` stream:
//!   salt || AEAD(len_be) || AEAD(payload) || ...

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use hkdf::Hkdf;
use md5::{Digest, Md5};
use sha1::Sha1;

pub const KEY_LEN: usize = 32;
pub const SALT_LEN: usize = 32;
pub const TAG_LEN: usize = 16;
pub const LENGTH_FRAME_LEN: usize = 2 + TAG_LEN;
pub const MAX_CHUNK: usize = 0x3fff;
const PASSWORD: &[u8] = b"password";
const SUBKEY_INFO: &[u8] = b"ss-subkey";

fn evp_bytes_to_key(password: &[u8]) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    let mut previous = Vec::new();
    let mut written = 0;

    while written < KEY_LEN {
        let mut hasher = Md5::new();
        if !previous.is_empty() {
            hasher.update(&previous);
        }
        hasher.update(password);
        previous = hasher.finalize().to_vec();

        let count = (KEY_LEN - written).min(previous.len());
        key[written..written + count].copy_from_slice(&previous[..count]);
        written += count;
    }
    key
}

fn derive_subkey(salt: &[u8; SALT_LEN]) -> Result<[u8; KEY_LEN], String> {
    let master = evp_bytes_to_key(PASSWORD);
    let hkdf = Hkdf::<Sha1>::new(Some(salt), &master);
    let mut subkey = [0u8; KEY_LEN];
    hkdf.expand(SUBKEY_INFO, &mut subkey)
        .map_err(|_| String::from("HKDF-SHA1 subkey expansion failed"))?;
    Ok(subkey)
}

fn increment_nonce(nonce: &mut [u8; 12]) {
    for byte in nonce {
        let (next, carry) = byte.overflowing_add(1);
        *byte = next;
        if !carry {
            break;
        }
    }
}

pub struct CipherState {
    cipher: ChaCha20Poly1305,
    nonce: [u8; 12],
}

impl CipherState {
    pub fn new(salt: &[u8; SALT_LEN]) -> Result<Self, String> {
        let subkey = derive_subkey(salt)?;
        Ok(Self {
            cipher: ChaCha20Poly1305::new((&subkey).into()),
            nonce: [0u8; 12],
        })
    }

    fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let nonce = Nonce::from(self.nonce);
        let output = self
            .cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| String::from("ChaCha20-Poly1305 encryption failed"))?;
        increment_nonce(&mut self.nonce);
        Ok(output)
    }

    fn open(&mut self, ciphertext_and_tag: &[u8]) -> Result<Vec<u8>, String> {
        if ciphertext_and_tag.len() < TAG_LEN {
            return Err(String::from("AEAD frame is shorter than its tag"));
        }
        let nonce = Nonce::from(self.nonce);
        let output = self
            .cipher
            .decrypt(&nonce, ciphertext_and_tag)
            .map_err(|_| String::from("ChaCha20-Poly1305 authentication failed"))?;
        increment_nonce(&mut self.nonce);
        Ok(output)
    }

    pub fn seal_chunk(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        if plaintext.len() > MAX_CHUNK {
            return Err(format!(
                "plaintext chunk {} exceeds maximum {MAX_CHUNK}",
                plaintext.len()
            ));
        }
        let mut output = self.seal(&(plaintext.len() as u16).to_be_bytes())?;
        output.extend_from_slice(&self.seal(plaintext)?);
        Ok(output)
    }

    pub fn open_length(&mut self, frame: &[u8]) -> Result<usize, String> {
        if frame.len() != LENGTH_FRAME_LEN {
            return Err(format!(
                "encrypted length frame has {} bytes, expected {LENGTH_FRAME_LEN}",
                frame.len()
            ));
        }
        let plain = self.open(frame)?;
        let length = u16::from_be_bytes([plain[0], plain[1]]) as usize;
        if length > MAX_CHUNK {
            return Err(format!("server chunk length {length} exceeds {MAX_CHUNK}"));
        }
        Ok(length)
    }

    pub fn open_payload(
        &mut self,
        frame: &[u8],
        expected_length: usize,
    ) -> Result<Vec<u8>, String> {
        if frame.len() != expected_length + TAG_LEN {
            return Err(format!(
                "encrypted payload has {} bytes, expected {}",
                frame.len(),
                expected_length + TAG_LEN
            ));
        }
        self.open(frame)
    }
}

pub fn random_salt() -> [u8; SALT_LEN] {
    rand::random()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evp_key_matches_known_password_vector() {
        assert_eq!(
            evp_bytes_to_key(b"password"),
            [
                0x5f, 0x4d, 0xcc, 0x3b, 0x5a, 0xa7, 0x65, 0xd6, 0x1d, 0x83, 0x27, 0xde, 0xb8, 0x82,
                0xcf, 0x99, 0x2b, 0x95, 0x99, 0x0a, 0x91, 0x51, 0x37, 0x4a, 0xbd, 0x8f, 0xf8, 0xc5,
                0xa7, 0xa0, 0xfe, 0x08,
            ]
        );
    }

    #[test]
    fn chunk_round_trip_and_nonce_progression() {
        let salt = [7u8; SALT_LEN];
        let mut encrypt = CipherState::new(&salt).unwrap();
        let mut decrypt = CipherState::new(&salt).unwrap();
        for plain in [b"hello".as_slice(), b"", &[42u8; 1024]] {
            let encrypted = encrypt.seal_chunk(plain).unwrap();
            let length = decrypt.open_length(&encrypted[..LENGTH_FRAME_LEN]).unwrap();
            let decoded = decrypt
                .open_payload(&encrypted[LENGTH_FRAME_LEN..], length)
                .unwrap();
            assert_eq!(decoded, plain);
        }
    }
}
