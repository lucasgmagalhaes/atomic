//! On-disk session encryption: AES-256-GCM via the `aes-gcm` crate (not a
//! hand-rolled cipher — same "don't reinvent crypto primitives" reasoning
//! the spec already applied to TLS in `net`). Real authenticated
//! encryption, not a stub: tampering with ciphertext or using the wrong
//! key both fail decryption rather than silently returning garbage.
//! Key/nonce randomness comes from `platform_apis::fill_random` (the OS
//! CSPRNG), the same primitive `js-runtime` uses for
//! `crypto.getRandomValues` — one source of randomness, not two.
//!
//! Also has real per-platform process sandboxing for a spawned
//! `profile-worker` — see [`sandbox`]. Not covered: a **signed updater**
//! (phase-5-scale effort on its own).
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};

pub mod sandbox;

pub const KEY_LEN: usize = 32; // AES-256
pub const NONCE_LEN: usize = 12; // GCM's standard nonce size

#[derive(Debug)]
pub enum Error {
    /// Wrong key, tampered ciphertext, or a truncated/corrupt blob -
    /// AES-GCM's authentication tag doesn't distinguish these, so
    /// neither do we.
    DecryptionFailed,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "decryption failed (wrong key or corrupted/tampered data)")
    }
}

impl std::error::Error for Error {}

/// Generates a fresh random 256-bit key from the OS CSPRNG. Callers are
/// responsible for storing it somewhere safe (the OS keychain, per the
/// spec's "Credential vault · Encrypted · OS keychain" note in the
/// mockup - not implemented here, this crate only does the encrypt/
/// decrypt math).
pub fn generate_key() -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    platform_apis::fill_random(&mut key).expect("OS CSPRNG should not fail");
    key
}

/// Encrypts `plaintext` under `key`. Returns `nonce || ciphertext`
/// (ciphertext includes AES-GCM's 16-byte authentication tag) — a single
/// self-contained blob callers can write straight to disk and hand back
/// to [`decrypt`] unmodified, without separately tracking the nonce.
/// Generates a fresh random nonce every call (required: reusing a nonce
/// with the same key breaks AES-GCM's security guarantees entirely).
pub fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Vec<u8> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    platform_apis::fill_random(&mut nonce_bytes).expect("OS CSPRNG should not fail");

    let cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(*key));
    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = cipher.encrypt(&nonce, plaintext).expect("AES-256-GCM encryption cannot fail for in-memory buffers");

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    out
}

/// Reverses [`encrypt`]. Fails if `blob` is too short to contain a
/// nonce, or if authentication fails (wrong key, or the blob was
/// tampered with / corrupted).
pub fn decrypt(key: &[u8; KEY_LEN], blob: &[u8]) -> Result<Vec<u8>, Error> {
    if blob.len() < NONCE_LEN {
        return Err(Error::DecryptionFailed);
    }
    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(*key));
    let nonce = Nonce::try_from(nonce_bytes).map_err(|_| Error::DecryptionFailed)?;
    cipher.decrypt(&nonce, ciphertext).map_err(|_| Error::DecryptionFailed)
}
