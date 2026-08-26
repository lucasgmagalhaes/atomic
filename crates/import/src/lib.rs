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

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A private, exclusively-created copy of a Chrome SQLite database. Chrome
/// may hold its original database open; the copy also prevents a concurrent
/// import from reading another operation's snapshot.
pub(crate) struct SqliteSnapshot {
  path: PathBuf,
}

impl SqliteSnapshot {
  pub(crate) fn path(&self) -> &Path {
    &self.path
  }
}

impl Drop for SqliteSnapshot {
  fn drop(&mut self) {
    let _ = std::fs::remove_file(&self.path);
  }
}

fn open_private_file(path: &Path) -> io::Result<File> {
  let mut options = OpenOptions::new();
  options.write(true).create_new(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
  }
  options.open(path)
}

/// Copies `source` to a fresh, owner-private temporary file. The filename is
/// unpredictable and `create_new` reserves it atomically, preventing both
/// snapshot collisions between concurrent imports and temp-file replacement.
pub(crate) fn snapshot_sqlite(source: &Path, label: &str) -> io::Result<SqliteSnapshot> {
  let mut input = File::open(source)?;
  for _ in 0..16 {
    let mut random = [0u8; 16];
    platform_apis::fill_random(&mut random).map_err(|error| {
      io::Error::new(
        io::ErrorKind::Other,
        format!("failed to create import snapshot name: {error}"),
      )
    })?;
    let mut suffix = String::with_capacity(random.len() * 2);
    for byte in random {
      use std::fmt::Write as _;
      write!(&mut suffix, "{byte:02x}").expect("writing to a String cannot fail");
    }
    let path = std::env::temp_dir().join(format!("nimble-import-{label}-{suffix}.sqlite"));
    let mut output = match open_private_file(&path) {
      Ok(file) => file,
      Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
      Err(error) => return Err(error),
    };

    let result = io::copy(&mut input, &mut output).and_then(|_| output.flush());
    drop(output);
    match result {
      Ok(()) => return Ok(SqliteSnapshot { path }),
      Err(error) => {
        let _ = std::fs::remove_file(&path);
        return Err(error);
      }
    }
  }
  Err(io::Error::new(
    io::ErrorKind::AlreadyExists,
    "could not reserve a unique import snapshot file",
  ))
}

pub use bookmarks::{import_bookmarks, Bookmark};
pub use cookies::{import_cookies, Cookie};
pub use history::{import_history, HistoryEntry};
pub use master_key::recover_master_key;
pub use passwords::{import_passwords, Password};
