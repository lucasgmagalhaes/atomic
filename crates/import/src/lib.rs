//! Real "Import from Chrome" — the mockup's onboarding flow CLAUDE.md
//! flagged as needing a decision (it conflicts with this project's "no
//! fingerprint spoofing" isolation requirement: pulling a real Chrome
//! profile's cookies/passwords into a nimble profile makes that profile
//! traceable back to the user's real browser). Decision made: full import,
//! **opt-in per profile** — nothing here runs unless a caller (`apps/shell`)
//! explicitly invokes it for one profile the user chose, never automatic,
//! never for every profile at once.
//!
//! [`bookmarks::import_bookmarks`] (Chrome `Bookmarks` JSON) and
//! [`history::import_history`] (Chrome `History` SQLite) are read-only, no
//! decryption needed — Chrome stores neither encrypted.
//!
//! Cookies (`Cookies` SQLite) and saved passwords (`Login Data` SQLite)
//! *are* encrypted — Chrome wraps an AES-256-GCM master key with Windows
//! DPAPI (`CryptUnprotectData`, real syscall, `master_key`/`dpapi`
//! modules) and stores it in `Local State`; each cookie/password value is
//! then AES-256-GCM-encrypted under that key (`encrypted_value`). Real,
//! working, Windows-only (DPAPI is a Windows-specific mechanism — macOS
//! Keychain/Linux Secret Service protect Chrome's key differently and
//! aren't implemented here). Doesn't cover `v20`/App-Bound Encryption
//! (Chrome 127+, wraps the key again behind an elevated helper process) —
//! a `v20`-prefixed value reports a real `UnsupportedVersion` error rather
//! than silently failing or returning garbage.
pub mod bookmarks;
pub mod chrome_profile;
pub mod cookies;
mod dpapi;
mod encrypted_value;
pub mod history;
mod json;
pub mod master_key;
pub mod passwords;

pub use bookmarks::{import_bookmarks, Bookmark};
pub use cookies::{import_cookies, Cookie};
pub use history::{import_history, HistoryEntry};
pub use master_key::recover_master_key;
pub use passwords::{import_passwords, Password};
