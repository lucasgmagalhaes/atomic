//! Credential vault UX — the mockup's "Credential vault · Encrypted · OS
//! keychain" settings row, wired to the real `security::CredentialVault`
//! (AES-256-GCM, see that module's own doc for the "not the real OS
//! keychain yet" deviation this inherits unchanged).
//!
//! One vault per shell process (not per profile - the mockup's "Add
//! profile modal" autofill fields aren't wired to this yet, so there's no
//! per-profile identity to key separate vaults off), living under
//! `%TEMP%/nimble-vault/` - same "dev-stage, not a real installed app data
//! dir" caveat `profile-worker`'s own storage_root already carries.
//!
//! No autofill wiring into pages - the mockup's own "Credentials
//! autofilled by profile" note has no form-field-detection counterpart
//! anywhere in this engine yet (`dom`/`js-runtime` have no concept of an
//! `<input>` distinct from any other element - see `automation`'s own
//! `fill` deviation for the same underlying gap). This is real storage
//! and a real UI to manage entries, not a real autofill feature.
use std::path::PathBuf;

use security::vault::CredentialVault;

pub fn default_vault_dir() -> PathBuf {
  std::env::temp_dir().join("nimble-vault")
}

/// Opens (creating if needed) the vault at `dir/vault.enc` with its key at
/// `dir/vault.key` - two files, matching `CredentialVault::open_or_create`'s
/// own convention of a separate plaintext key file next to the encrypted
/// vault (see that module's doc on why that's not real OS-keychain
/// integration).
pub fn open(dir: &std::path::Path) -> std::io::Result<CredentialVault> {
  CredentialVault::open_or_create(dir.join("vault.enc"), dir.join("vault.key"))
}

/// Same as [`open`], but the master key is sourced from the real OS
/// credential store instead of a plaintext file next to the vault (see
/// `CredentialVault::open_or_create_with_keychain`'s own doc) - closes the
/// "OS keychain" deviation for real, where the platform has a backend
/// (Windows only so far - `Err` on any other platform, no silent fallback
/// to the file-backed vault).
///
/// Deliberately a *separate* file (`dir/vault-keychain.enc`, not
/// `vault.enc`) rather than the same file with a swapped key source: the
/// two modes use genuinely different encryption keys, so pointing them at
/// one file would make switching modes look like data loss (a real decrypt
/// failure against whatever key the other mode expects) instead of what it
/// actually is - two independent vaults, not a migration between them.
pub fn open_with_keychain(dir: &std::path::Path) -> std::io::Result<CredentialVault> {
  CredentialVault::open_or_create_with_keychain(dir.join("vault-keychain.enc"))
}
