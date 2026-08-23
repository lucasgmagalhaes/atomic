//! Signed updater: verifies an update artifact's SHA-256 hash against a
//! manifest, and the manifest's Ed25519 signature against the app's
//! embedded public key, before swapping the running binary for the new
//! one. The same mechanism most non-store auto-updaters use (Squirrel,
//! Sparkle, `electron-updater`) — not OS-level code signing (Windows
//! Authenticode / macOS notarization), which needs a CA-issued certificate
//! this project doesn't have and is a separate, orthogonal mechanism (an
//! Authenticode-signed binary can still be a *stale* or *malicious*
//! binary if nothing checks what update fed it there; this crate is that
//! check).
//!
//! Real primitives throughout: `ed25519-dalek` for the signature (not a
//! hand-rolled scheme — same "don't reinvent crypto primitives" reasoning
//! applied to AES-GCM in this crate and TLS in `net`), `sha2` for the
//! artifact hash, `platform_apis::fill_random` (the OS CSPRNG) for key
//! generation.
//!
//! Scope cuts: no update *feed* (fetching a manifest URL, checking a
//! version against "currently installed" — that's `net` plus a policy
//! layer, not this crate's job), no delta/binary-diff patching (whole-
//! artifact replacement only), no rollback-on-failed-launch (a corrupt
//! update that still passes signature/hash checks — e.g. a bug in the
//! build that produced it — would need a "did the new binary actually
//! start" heartbeat this project has no launcher for yet).
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

pub const PUBLIC_KEY_LEN: usize = 32;
pub const SECRET_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The manifest's signature doesn't verify against the given public
    /// key — wrong key, corrupted manifest, or a tampered/forged update.
    BadSignature,
    /// The artifact's actual SHA-256 doesn't match what the (validly
    /// signed) manifest claims — the artifact bytes were swapped or
    /// corrupted after the manifest was signed.
    HashMismatch,
    /// The manifest bytes aren't in the format [`Manifest::parse`] expects.
    MalformedManifest,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::BadSignature => write!(f, "manifest signature verification failed"),
            Error::HashMismatch => write!(f, "artifact hash does not match the signed manifest"),
            Error::MalformedManifest => write!(f, "manifest is not in the expected format"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// Generates a fresh Ed25519 keypair from the OS CSPRNG. The signing key
/// stays with whoever builds releases (never ships in the app); the
/// verifying key is embedded in the app to check updates against.
pub fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let mut seed = [0u8; SECRET_KEY_LEN];
    platform_apis::fill_random(&mut seed).expect("OS CSPRNG should not fail");
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

/// What an update claims about itself: a version string and the SHA-256 of
/// the artifact it describes. Deliberately not the artifact's *contents* —
/// this manifest is small and cheap to sign/verify/transmit, only the hash
/// ties it to the (potentially large) real download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub version: String,
    pub sha256: [u8; 32],
}

impl Manifest {
    /// Deterministic wire format this manifest is signed over:
    /// `"<version>\n<sha256 as lowercase hex>\n"`. Hand-rolled rather than
    /// pulled in via serde (matches this workspace's existing "no serde"
    /// convention for small fixed-shape formats — see `storage`'s
    /// newline-delimited files) — but unlike those, a signed manifest's
    /// wire format must be exactly reproducible byte-for-byte on both the
    /// signing and verifying side, so the format is spelled out here
    /// explicitly rather than left to a `Display` impl callers might
    /// accidentally diverge from.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut hex = String::with_capacity(64);
        for byte in self.sha256 {
            hex.push_str(&format!("{byte:02x}"));
        }
        format!("{}\n{}\n", self.version, hex).into_bytes()
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let text = std::str::from_utf8(bytes).map_err(|_| Error::MalformedManifest)?;
        let mut lines = text.lines();
        let version = lines.next().ok_or(Error::MalformedManifest)?.to_string();
        let hex = lines.next().ok_or(Error::MalformedManifest)?;
        if hex.len() != 64 {
            return Err(Error::MalformedManifest);
        }
        let mut sha256 = [0u8; 32];
        for (i, byte) in sha256.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| Error::MalformedManifest)?;
        }
        Ok(Manifest { version, sha256 })
    }
}

/// Real SHA-256 over `path`'s contents, streamed rather than loaded whole
/// (an update artifact could be a multi-hundred-MB installer).
pub fn hash_file(path: impl AsRef<Path>) -> io::Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().into())
}

/// Builds and signs a manifest for the artifact at `artifact_path`.
pub fn sign_artifact(signing_key: &SigningKey, version: &str, artifact_path: impl AsRef<Path>) -> io::Result<(Manifest, Signature)> {
    let sha256 = hash_file(artifact_path)?;
    let manifest = Manifest { version: version.to_string(), sha256 };
    let signature = signing_key.sign(&manifest.to_bytes());
    Ok((manifest, signature))
}

/// Verifies `manifest` was signed by the holder of `verifying_key`.
pub fn verify_manifest(verifying_key: &VerifyingKey, manifest: &Manifest, signature: &Signature) -> Result<(), Error> {
    verifying_key.verify(&manifest.to_bytes(), signature).map_err(|_| Error::BadSignature)
}

/// The full update flow: verify `manifest`'s signature, verify
/// `artifact_path`'s actual SHA-256 matches what the (now-trusted)
/// manifest claims, then atomically replace `install_path` with the
/// artifact — `fs::rename`, same-volume swap, so there's no window where
/// `install_path` is missing or half-written. Callers must ensure
/// `artifact_path` and `install_path` are on the same filesystem volume
/// (an `fs::rename` across volumes fails; this function doesn't fall back
/// to copy+delete, since that reintroduces the half-written-file window
/// this is trying to avoid).
pub fn apply_update(
    verifying_key: &VerifyingKey,
    manifest: &Manifest,
    signature: &Signature,
    artifact_path: impl AsRef<Path>,
    install_path: impl AsRef<Path>,
) -> Result<(), Error> {
    verify_manifest(verifying_key, manifest, signature)?;

    let actual_hash = hash_file(&artifact_path)?;
    if actual_hash != manifest.sha256 {
        return Err(Error::HashMismatch);
    }

    fs::rename(artifact_path, install_path)?;
    Ok(())
}
