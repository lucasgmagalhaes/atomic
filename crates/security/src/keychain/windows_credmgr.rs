//! Real Windows Credential Manager backing for a vault's master key —
//! `CredWriteW`/`CredReadW`/`CredDeleteW` against a generic credential
//! (`CRED_TYPE_GENERIC`), persisted `CRED_PERSIST_LOCAL_MACHINE` so it
//! survives reboot the same way a file-based key does. Real syscalls
//! against the actual OS credential store on this machine, not a stub —
//! same convention as `sandbox::windows_job` and
//! `platform_apis::process_stats`.
use std::io;

use windows_sys::core::PWSTR;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND, FALSE};
use windows_sys::Win32::Security::Credentials::{
  CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
  CRED_TYPE_GENERIC,
};

#[derive(Debug)]
pub enum KeychainError {
  /// No credential exists under this target name yet.
  NotFound,
  WriteFailed(io::Error),
  ReadFailed(io::Error),
  DeleteFailed(io::Error),
}

impl std::fmt::Display for KeychainError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      KeychainError::NotFound => write!(f, "no credential found under this target name"),
      KeychainError::WriteFailed(e) => write!(f, "CredWriteW failed: {e}"),
      KeychainError::ReadFailed(e) => write!(f, "CredReadW failed: {e}"),
      KeychainError::DeleteFailed(e) => write!(f, "CredDeleteW failed: {e}"),
    }
  }
}

impl std::error::Error for KeychainError {}

fn to_wide_null(s: &str) -> Vec<u16> {
  s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Writes `secret` (arbitrary bytes, e.g. a 32-byte AES key) to the real
/// Windows Credential Manager under `target`, as a generic credential
/// persisted at the local-machine level.
pub fn write_credential(target: &str, secret: &[u8]) -> Result<(), KeychainError> {
  let mut target_wide = to_wide_null(target);
  let mut blob = secret.to_vec();

  let credential = CREDENTIALW {
    Flags: 0,
    Type: CRED_TYPE_GENERIC,
    TargetName: target_wide.as_mut_ptr(),
    Comment: std::ptr::null_mut(),
    LastWritten: unsafe { std::mem::zeroed() },
    CredentialBlobSize: blob.len() as u32,
    CredentialBlob: blob.as_mut_ptr(),
    Persist: CRED_PERSIST_LOCAL_MACHINE,
    AttributeCount: 0,
    Attributes: std::ptr::null_mut(),
    TargetAlias: std::ptr::null_mut(),
    UserName: std::ptr::null_mut(),
  };

  let ok = unsafe { CredWriteW(&credential, 0) };
  if ok == FALSE {
    return Err(KeychainError::WriteFailed(io::Error::last_os_error()));
  }
  Ok(())
}

/// Reads back whatever [`write_credential`] stored under `target`.
/// Returns [`KeychainError::NotFound`] specifically when no such
/// credential exists yet, distinct from other OS-level failures.
pub fn read_credential(target: &str) -> Result<Vec<u8>, KeychainError> {
  let target_wide = to_wide_null(target);
  let mut credential_ptr: *mut CREDENTIALW = std::ptr::null_mut();

  let ok = unsafe {
    CredReadW(
      target_wide.as_ptr() as PWSTR,
      CRED_TYPE_GENERIC,
      0,
      &mut credential_ptr,
    )
  };
  if ok == FALSE {
    let err = unsafe { GetLastError() };
    return Err(if err == ERROR_NOT_FOUND {
      KeychainError::NotFound
    } else {
      KeychainError::ReadFailed(io::Error::from_raw_os_error(err as i32))
    });
  }

  let bytes = unsafe {
    let credential = &*credential_ptr;
    std::slice::from_raw_parts(
      credential.CredentialBlob,
      credential.CredentialBlobSize as usize,
    )
    .to_vec()
  };
  unsafe { CredFree(credential_ptr as *const std::ffi::c_void) };
  Ok(bytes)
}

/// Deletes the credential stored under `target`, if any. Not an error if
/// it was already absent — matches `remove`-style idempotence elsewhere
/// in this workspace (e.g. `storage::LocalStorage::remove_item`).
pub fn delete_credential(target: &str) -> Result<(), KeychainError> {
  let target_wide = to_wide_null(target);
  let ok = unsafe { CredDeleteW(target_wide.as_ptr() as PWSTR, CRED_TYPE_GENERIC, 0) };
  if ok == FALSE {
    let err = unsafe { GetLastError() };
    if err == ERROR_NOT_FOUND {
      return Ok(());
    }
    return Err(KeychainError::DeleteFailed(io::Error::from_raw_os_error(
      err as i32,
    )));
  }
  Ok(())
}
