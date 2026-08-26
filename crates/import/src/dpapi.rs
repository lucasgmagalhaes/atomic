//! Real Windows DPAPI unwrap (`CryptUnprotectData`) - the same primitive
//! Chrome itself uses to protect its AES master key at rest in `Local
//! State`. Windows only (real, different mechanism per platform - macOS
//! Keychain / Linux Secret Service store Chrome's key differently and
//! aren't implemented here, same per-platform-real-or-`Unsupported`
//! convention as `security::sandbox`/`security::keychain`).
#[derive(Debug)]
pub enum DpapiError {
  #[cfg(windows)]
  Unwrap(std::io::Error),
  Unsupported,
}

impl std::fmt::Display for DpapiError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      #[cfg(windows)]
      DpapiError::Unwrap(e) => write!(f, "CryptUnprotectData failed: {e}"),
      DpapiError::Unsupported => write!(f, "DPAPI is only implemented on Windows"),
    }
  }
}

impl std::error::Error for DpapiError {}

#[cfg(windows)]
pub fn unprotect(encrypted: &[u8]) -> Result<Vec<u8>, DpapiError> {
  use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

  // `CryptUnprotectData` writes through these output pointers - both
  // start zeroed/null so a failure path never frees or reads garbage.
  let mut in_blob = CRYPT_INTEGER_BLOB {
    cbData: encrypted.len() as u32,
    pbData: encrypted.as_ptr() as *mut u8,
  };
  let mut out_blob = CRYPT_INTEGER_BLOB {
    cbData: 0,
    pbData: std::ptr::null_mut(),
  };

  // Safety: `in_blob` points at `encrypted`, alive for this whole call;
  // `out_blob` is an out-parameter the OS allocates into via `LocalAlloc`
  // internally - freed below via `LocalFree`, the real documented
  // contract for this API's output blob (same "OS owns this until we
  // free it" shape as `CredReadW`'s `CREDENTIALW*` in
  // `security::keychain`).
  let ok = unsafe {
    CryptUnprotectData(
      &mut in_blob,
      std::ptr::null_mut(),
      std::ptr::null_mut(),
      std::ptr::null_mut(),
      std::ptr::null_mut(),
      0,
      &mut out_blob,
    )
  };
  if ok == 0 {
    return Err(DpapiError::Unwrap(std::io::Error::last_os_error()));
  }

  let bytes =
    unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
  unsafe {
    windows_sys::Win32::Foundation::LocalFree(out_blob.pbData as *mut core::ffi::c_void);
  }
  Ok(bytes)
}

#[cfg(not(windows))]
pub fn unprotect(_encrypted: &[u8]) -> Result<Vec<u8>, DpapiError> {
  Err(DpapiError::Unsupported)
}
