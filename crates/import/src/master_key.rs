//! Real Chrome master-key recovery: `Local State` (JSON, one directory up
//! from a profile - e.g. `...\User Data\Local State`, a sibling of
//! `Default\`) stores `os_crypt.encrypted_key`, base64 of a `"DPAPI"`
//! 5-byte prefix + the actual DPAPI-wrapped AES-256 key. Real algorithm
//! Chrome itself uses (documented in Chromium's own `os_crypt_win.cc`),
//! not guessed.
use crate::dpapi::{self, DpapiError};
use crate::json::{parse, Json, JsonParseError};
use base64::Engine;
use std::path::Path;

#[derive(Debug)]
pub enum MasterKeyError {
  Io(std::io::Error),
  Parse(JsonParseError),
  MissingKey,
  Base64,
  /// The decoded bytes don't start with the real `"DPAPI"` prefix every
  /// actual Chrome `Local State` file has - a genuine format mismatch,
  /// not something to silently paper over.
  MissingDpapiPrefix,
  Dpapi(DpapiError),
}

impl std::fmt::Display for MasterKeyError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      MasterKeyError::Io(e) => write!(f, "failed to read Local State: {e}"),
      MasterKeyError::Parse(e) => write!(f, "failed to parse Local State: {e}"),
      MasterKeyError::MissingKey => write!(f, "Local State has no os_crypt.encrypted_key"),
      MasterKeyError::Base64 => write!(f, "os_crypt.encrypted_key isn't valid base64"),
      MasterKeyError::MissingDpapiPrefix => write!(
        f,
        "encrypted_key doesn't start with the real \"DPAPI\" prefix"
      ),
      MasterKeyError::Dpapi(e) => write!(f, "{e}"),
    }
  }
}

impl std::error::Error for MasterKeyError {}

const DPAPI_PREFIX: &[u8] = b"DPAPI";

/// `user_data_dir` is Chrome's top-level `User Data` directory (the parent
/// of `Default`/other profile directories, not a profile directory
/// itself) - `Local State` lives there, shared across every profile.
pub fn recover_master_key(user_data_dir: &Path) -> Result<Vec<u8>, MasterKeyError> {
  let text =
    std::fs::read_to_string(user_data_dir.join("Local State")).map_err(MasterKeyError::Io)?;
  let root = parse(&text).map_err(MasterKeyError::Parse)?;
  let encoded = root
    .get("os_crypt")
    .and_then(|c| c.get("encrypted_key"))
    .and_then(Json::as_str)
    .ok_or(MasterKeyError::MissingKey)?;
  let decoded = base64::engine::general_purpose::STANDARD
    .decode(encoded)
    .map_err(|_| MasterKeyError::Base64)?;
  let wrapped = decoded
    .strip_prefix(DPAPI_PREFIX)
    .ok_or(MasterKeyError::MissingDpapiPrefix)?;
  dpapi::unprotect(wrapped).map_err(MasterKeyError::Dpapi)
}
