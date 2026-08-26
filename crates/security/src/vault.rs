//! Credential vault — the spec's "Credential vault · Encrypted · OS
//! keychain" mockup note, closing the gap CLAUDE.md flags ("no
//! equivalent on the spec"). Real AES-256-GCM encryption of every
//! entry (reuses [`crate::encrypt`]/[`crate::decrypt`], the exact same
//! primitive `security::updater` and the rest of this crate already
//! trust), a real per-vault master key generated from the OS CSPRNG
//! (`crate::generate_key`), and a real persisted on-disk file — not an
//! in-memory placeholder.
//!
//! Deviation from "OS keychain": the master key itself is stored as a
//! plain (unencrypted) file next to the vault, not handed off to the
//! OS's real credential store (Windows Credential Manager / macOS
//! Keychain / Linux Secret Service) — that's a real per-platform
//! integration this crate doesn't have yet, tracked the same way
//! `sandbox`'s per-platform gap already is. Anyone who can read the key
//! file can decrypt the vault; what this *does* provide is real
//! at-rest encryption against anything that only has the vault file
//! itself (e.g. a stray backup/copy of one file without the other).
//!
//! One flat key→value map per vault file (e.g. one vault per profile),
//! not one entry per credential kind — a caller picks its own key
//! naming convention (e.g. `"login.email"`/`"login.password"` for the
//! mockup's autofill fields). Values are opaque UTF-8 strings, not a
//! typed credential record.
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use base64::Engine;

use crate::keychain;
use crate::{decrypt, encrypt, generate_key, Error, KEY_LEN};

pub struct CredentialVault {
  path: PathBuf,
  key: [u8; KEY_LEN],
  entries: BTreeMap<String, String>,
}

impl CredentialVault {
  /// Opens the vault at `path`, creating both it and `key_path` (the
  /// master key file) if either is missing. A missing vault file
  /// starts empty; a missing key file gets a fresh
  /// [`crate::generate_key`] written to it — so the *first* call to
  /// open a given `(path, key_path)` pair effectively provisions the
  /// vault.
  pub fn open_or_create(path: impl AsRef<Path>, key_path: impl AsRef<Path>) -> io::Result<Self> {
    let path = path.as_ref().to_path_buf();
    let key_path = key_path.as_ref();

    let key = match std::fs::read(key_path) {
      Ok(bytes) => {
        let arr: [u8; KEY_LEN] = bytes.try_into().map_err(|_| {
          io::Error::new(
            io::ErrorKind::InvalidData,
            "vault key file has the wrong length",
          )
        })?;
        arr
      }
      Err(e) if e.kind() == io::ErrorKind::NotFound => {
        let key = generate_key();
        if let Some(parent) = key_path.parent() {
          std::fs::create_dir_all(parent)?;
        }
        std::fs::write(key_path, key)?;
        key
      }
      Err(e) => return Err(e),
    };

    let entries = match std::fs::read(&path) {
      Ok(blob) if !blob.is_empty() => {
        let plaintext = decrypt(&key, &blob).map_err(|Error::DecryptionFailed| {
          io::Error::new(
            io::ErrorKind::InvalidData,
            "vault file failed to decrypt (wrong key or corrupted)",
          )
        })?;
        parse_entries(&plaintext)
      }
      Ok(_) => BTreeMap::new(),
      Err(e) if e.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
      Err(e) => return Err(e),
    };

    Ok(CredentialVault { path, key, entries })
  }

  /// Same as [`open_or_create`](Self::open_or_create), except the master
  /// key is sourced from and stored in the real OS credential store
  /// (Windows Credential Manager — see [`crate::keychain`]) instead of a
  /// plain file, closing the "OS keychain" deviation this module's own
  /// doc comment flags. The credential's target name is derived from
  /// `path` itself (`nimble-vault:<absolute path>`), so a distinct vault
  /// file gets a distinct keychain entry without the caller having to
  /// pick a name.
  ///
  /// Returns `Err` (not `Ok` with a fallback) on a platform without a
  /// real [`crate::keychain`] backend, or if the OS credential store
  /// itself rejects the read/write — same "no fake behavior" convention
  /// as [`crate::sandbox::confine`].
  pub fn open_or_create_with_keychain(path: impl AsRef<Path>) -> io::Result<Self> {
    let path = path.as_ref().to_path_buf();
    let target = keychain_target_for(&path);

    let key = match keychain::read_credential(&target) {
      Ok(bytes) => bytes.try_into().map_err(|_| {
        io::Error::new(
          io::ErrorKind::InvalidData,
          "keychain credential has the wrong length for a vault key",
        )
      })?,
      Err(e) if is_not_found(&e) => {
        let key = generate_key();
        keychain::write_credential(&target, &key).map_err(keychain_io_error)?;
        key
      }
      Err(e) => return Err(keychain_io_error(e)),
    };

    let entries = match std::fs::read(&path) {
      Ok(blob) if !blob.is_empty() => {
        let plaintext = decrypt(&key, &blob).map_err(|Error::DecryptionFailed| {
          io::Error::new(
            io::ErrorKind::InvalidData,
            "vault file failed to decrypt (wrong key or corrupted)",
          )
        })?;
        parse_entries(&plaintext)
      }
      Ok(_) => BTreeMap::new(),
      Err(e) if e.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
      Err(e) => return Err(e),
    };

    Ok(CredentialVault { path, key, entries })
  }

  pub fn get(&self, key: &str) -> Option<&str> {
    self.entries.get(key).map(String::as_str)
  }

  /// Sets `key` to `value` and immediately re-encrypts + flushes the
  /// whole vault to disk (same "flush every mutation" convention as
  /// `storage::LocalStorage`).
  pub fn set(&mut self, key: &str, value: &str) -> io::Result<()> {
    self.entries.insert(key.to_string(), value.to_string());
    self.flush()
  }

  pub fn remove(&mut self, key: &str) -> io::Result<()> {
    self.entries.remove(key);
    self.flush()
  }

  pub fn keys(&self) -> impl Iterator<Item = &str> {
    self.entries.keys().map(String::as_str)
  }

  fn flush(&self) -> io::Result<()> {
    let plaintext = serialize_entries(&self.entries);
    let blob = encrypt(&self.key, &plaintext);
    if let Some(parent) = self.path.parent() {
      std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&self.path, blob)
  }
}

/// One `base64(key) SP base64(value)` pair per line — base64 sidesteps
/// needing to escape whatever bytes a key/value contains (matches
/// `net::proxy`'s own use of base64 for a similar "opaque bytes as one
/// text line" problem), simpler than a real parser for this crate's
/// scope.
fn serialize_entries(entries: &BTreeMap<String, String>) -> Vec<u8> {
  let engine = base64::engine::general_purpose::STANDARD;
  let mut out = String::new();
  for (k, v) in entries {
    out.push_str(&engine.encode(k));
    out.push(' ');
    out.push_str(&engine.encode(v));
    out.push('\n');
  }
  out.into_bytes()
}

/// Deterministic keychain target name for a given vault file — one
/// credential per distinct vault path.
fn keychain_target_for(path: &Path) -> String {
  format!("nimble-vault:{}", path.display())
}

#[cfg(windows)]
fn is_not_found(e: &keychain::KeychainError) -> bool {
  matches!(e, keychain::KeychainError::NotFound)
}

#[cfg(not(windows))]
fn is_not_found(_e: &keychain::KeychainError) -> bool {
  false
}

fn keychain_io_error(e: keychain::KeychainError) -> io::Error {
  io::Error::new(io::ErrorKind::Other, e.to_string())
}

fn parse_entries(plaintext: &[u8]) -> BTreeMap<String, String> {
  let engine = base64::engine::general_purpose::STANDARD;
  let text = String::from_utf8_lossy(plaintext);
  let mut map = BTreeMap::new();
  for line in text.lines() {
    let Some((k, v)) = line.split_once(' ') else {
      continue;
    };
    let (Ok(k), Ok(v)) = (engine.decode(k), engine.decode(v)) else {
      continue;
    };
    let (Ok(k), Ok(v)) = (String::from_utf8(k), String::from_utf8(v)) else {
      continue;
    };
    map.insert(k, v);
  }
  map
}
