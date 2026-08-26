//! Real import of Chrome's `Cookies` file (SQLite, under a profile
//! directory - a sibling of `History`/`Bookmarks`) with real decryption:
//! `encrypted_value` is AES-256-GCM under the real DPAPI-unwrapped master
//! key (see `master_key`/`encrypted_value`), the actually hard half of
//! "full import" this crate's own doc flags as not attempted in an
//! earlier pass - now it is.
use crate::encrypted_value;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct Cookie {
  pub host: String,
  pub name: String,
  pub value: String,
  pub path: String,
  pub is_secure: bool,
  pub is_http_only: bool,
  pub expires: Option<SystemTime>,
}

#[derive(Debug)]
pub enum ImportError {
  Io(std::io::Error),
  Sqlite(rusqlite::Error),
}

impl std::fmt::Display for ImportError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ImportError::Io(e) => write!(f, "failed to prepare cookies file for import: {e}"),
      ImportError::Sqlite(e) => write!(f, "failed to read cookies database: {e}"),
    }
  }
}

impl std::error::Error for ImportError {}

/// Same WebKit-epoch offset `history.rs` uses - Chrome's `Cookies` table
/// stores `expires_utc` in the same units as `History`'s `last_visit_time`.
const WEBKIT_EPOCH_OFFSET_MICROS: i64 = 11_644_473_600_000_000;

fn webkit_time_to_system_time(webkit_micros: i64) -> Option<SystemTime> {
  if webkit_micros == 0 {
    return None; // Chrome's own "no expiry" sentinel for session cookies.
  }
  let unix_micros = webkit_micros - WEBKIT_EPOCH_OFFSET_MICROS;
  Some(if unix_micros >= 0 {
    UNIX_EPOCH + Duration::from_micros(unix_micros as u64)
  } else {
    UNIX_EPOCH - Duration::from_micros((-unix_micros) as u64)
  })
}

/// Reads every real row from `path`'s `cookies` table and decrypts
/// `encrypted_value` with `master_key` - a row whose value fails to
/// decrypt (wrong key, unsupported `v20` scheme) is skipped rather than
/// failing the whole import, since one bad cookie shouldn't lose every
/// other real one.
pub fn import_cookies(path: &Path, master_key: &[u8]) -> Result<Vec<Cookie>, ImportError> {
  let snapshot = crate::snapshot_sqlite(path, "cookies").map_err(ImportError::Io)?;

  (|| {
    let conn = rusqlite::Connection::open(snapshot.path()).map_err(ImportError::Sqlite)?;
    let mut stmt = conn
            .prepare("SELECT host_key, name, encrypted_value, path, is_secure, is_httponly, expires_utc FROM cookies")
            .map_err(ImportError::Sqlite)?;
    let rows = stmt
      .query_map([], |row| {
        let host: String = row.get(0)?;
        let name: String = row.get(1)?;
        let encrypted: Vec<u8> = row.get(2)?;
        let path: String = row.get(3)?;
        let is_secure: i64 = row.get(4)?;
        let is_http_only: i64 = row.get(5)?;
        let expires_utc: i64 = row.get(6)?;
        Ok((
          host,
          name,
          encrypted,
          path,
          is_secure != 0,
          is_http_only != 0,
          expires_utc,
        ))
      })
      .map_err(ImportError::Sqlite)?;

    let mut cookies = Vec::new();
    for row in rows {
      let (host, name, encrypted, path, is_secure, is_http_only, expires_utc) =
        row.map_err(ImportError::Sqlite)?;
      if let Ok(value) = encrypted_value::decrypt(master_key, &encrypted) {
        cookies.push(Cookie {
          host,
          name,
          value,
          path,
          is_secure,
          is_http_only,
          expires: webkit_time_to_system_time(expires_utc),
        });
      }
    }
    Ok(cookies)
  })()
}
