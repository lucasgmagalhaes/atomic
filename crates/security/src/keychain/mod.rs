//! OS keychain backing for the vault's master key (closes the "OS
//! keychain" deviation `vault`'s own doc comment flags) — same
//! per-platform-real-or-`Unsupported` convention as [`crate::sandbox`]:
//! Windows Credential Manager is implemented via real `CredWriteW`/
//! `CredReadW`/`CredDeleteW` syscalls, Linux (Secret Service over D-Bus)
//! and macOS (Keychain Services) are real, different APIs that need
//! their own machine to write and test against — not implemented here.
#[cfg(windows)]
mod windows_credmgr;
#[cfg(windows)]
pub use windows_credmgr::{delete_credential, read_credential, write_credential, KeychainError};

#[cfg(not(windows))]
mod unsupported {
  #[derive(Debug)]
  pub enum KeychainError {
    Unsupported,
  }

  impl std::fmt::Display for KeychainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      write!(
        f,
        "OS keychain access is not implemented on this platform yet"
      )
    }
  }

  impl std::error::Error for KeychainError {}

  pub fn write_credential(_target: &str, _secret: &[u8]) -> Result<(), KeychainError> {
    Err(KeychainError::Unsupported)
  }

  pub fn read_credential(_target: &str) -> Result<Vec<u8>, KeychainError> {
    Err(KeychainError::Unsupported)
  }

  pub fn delete_credential(_target: &str) -> Result<(), KeychainError> {
    Err(KeychainError::Unsupported)
  }
}
#[cfg(not(windows))]
pub use unsupported::{delete_credential, read_credential, write_credential, KeychainError};
