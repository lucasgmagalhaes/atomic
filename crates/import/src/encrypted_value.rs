//! Decrypts one Chrome `encrypted_value` blob (the `cookies.encrypted_value`
//! / `logins.password_value` column format) - real AES-256-GCM against the
//! master key `master_key::recover_master_key` returns, matching Chrome's
//! own `v10`/`v11` scheme: 3-byte version prefix, 12-byte GCM nonce, then
//! ciphertext+16-byte tag. `v20`/App-Bound Encryption (Chrome 127+, wraps
//! the key again behind an elevated helper service) is a different, newer
//! mechanism this doesn't implement - a `v20`-prefixed value reports a
//! real `UnsupportedVersion` error rather than silently returning garbage.
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit};

#[derive(Debug)]
pub enum DecryptError {
    TooShort,
    UnsupportedVersion(String),
    BadKeyLength,
    AuthenticationFailed,
    Utf8,
}

impl std::fmt::Display for DecryptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecryptError::TooShort => write!(f, "encrypted value is too short to contain a real version prefix + nonce"),
            DecryptError::UnsupportedVersion(v) => write!(f, "unsupported encrypted-value version {v:?} (only v10/v11 are implemented)"),
            DecryptError::BadKeyLength => write!(f, "master key isn't a real 32-byte AES-256 key"),
            DecryptError::AuthenticationFailed => write!(f, "GCM authentication failed - wrong key or corrupted value"),
            DecryptError::Utf8 => write!(f, "decrypted bytes aren't valid UTF-8"),
        }
    }
}

impl std::error::Error for DecryptError {}

const NONCE_LEN: usize = 12;

pub fn decrypt(master_key: &[u8], blob: &[u8]) -> Result<String, DecryptError> {
    if blob.len() < 3 + NONCE_LEN {
        return Err(DecryptError::TooShort);
    }
    let version = &blob[0..3];
    if version != b"v10" && version != b"v11" {
        return Err(DecryptError::UnsupportedVersion(String::from_utf8_lossy(version).to_string()));
    }
    let nonce: &[u8; NONCE_LEN] = blob[3..3 + NONCE_LEN].try_into().expect("slice length checked above");
    let ciphertext = &blob[3 + NONCE_LEN..];

    let cipher = Aes256Gcm::new_from_slice(master_key).map_err(|_| DecryptError::BadKeyLength)?;
    let plaintext = cipher.decrypt(nonce.into(), ciphertext).map_err(|_| DecryptError::AuthenticationFailed)?;
    String::from_utf8(plaintext).map_err(|_| DecryptError::Utf8)
}
