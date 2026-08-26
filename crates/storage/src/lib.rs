//! `localStorage`/`sessionStorage`-shaped key-value storage, persisted to
//! disk per origin — real file I/O, not an in-memory stub. Also has real
//! cookie handling ([`cookies`]) and a scoped-down IndexedDB
//! ([`indexed_db`]) — see their module docs for what's cut. That closes
//! the spec's full "cookies/localStorage/sessionStorage/IndexedDB" line,
//! though not every corner of each piece.
//!
//! `sessionStorage`'s real-spec lifetime (cleared when the tab/process
//! ends) isn't modeled either — `SessionStorage` here is just
//! `LocalStorage` under a different constructor name, persisted the same
//! way. The actual "clear on process exit" behavior would come from
//! `profile` deciding whether to persist a profile's session storage
//! file at all, not from this crate.
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub mod cookies;
pub mod indexed_db;
pub mod value;

/// A flat string-keyed store for one origin, backed by one file on disk.
/// Every mutation persists immediately (no write batching/debouncing) -
/// simple and correct, though not the fastest possible under heavy
/// churn; revisit if that ever matters.
pub struct LocalStorage {
  path: PathBuf,
  entries: HashMap<String, String>,
}

/// Escapes `\` and `\n` so entries can round-trip through a
/// newline-delimited file format without ambiguity - not a general
/// serialization format, just enough for flat string keys/values.
pub(crate) fn escape(s: &str) -> String {
  s.replace('\\', "\\\\").replace('\n', "\\n")
}

pub(crate) fn unescape(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut chars = s.chars();
  while let Some(c) = chars.next() {
    if c == '\\' {
      match chars.next() {
        Some('n') => out.push('\n'),
        Some('\\') => out.push('\\'),
        Some(other) => {
          out.push('\\');
          out.push(other);
        }
        None => out.push('\\'),
      }
    } else {
      out.push(c);
    }
  }
  out
}

impl LocalStorage {
  /// Opens (loading existing entries, if any) the store backed by
  /// `path`. Creates the file's parent directory if needed; the file
  /// itself is created lazily on first write.
  pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
    let path = path.as_ref().to_path_buf();
    let mut entries = HashMap::new();

    if let Ok(contents) = fs::read_to_string(&path) {
      let mut lines = contents.lines();
      while let (Some(k), Some(v)) = (lines.next(), lines.next()) {
        entries.insert(unescape(k), unescape(v));
      }
    }

    Ok(LocalStorage { path, entries })
  }

  pub fn get(&self, key: &str) -> Option<&str> {
    self.entries.get(key).map(String::as_str)
  }

  pub fn set(&mut self, key: &str, value: &str) -> io::Result<()> {
    self.entries.insert(key.to_string(), value.to_string());
    self.flush()
  }

  pub fn remove(&mut self, key: &str) -> io::Result<()> {
    self.entries.remove(key);
    self.flush()
  }

  pub fn clear(&mut self) -> io::Result<()> {
    self.entries.clear();
    self.flush()
  }

  pub fn len(&self) -> usize {
    self.entries.len()
  }

  pub fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }

  pub fn keys(&self) -> impl Iterator<Item = &str> {
    self.entries.keys().map(String::as_str)
  }

  fn flush(&self) -> io::Result<()> {
    if let Some(parent) = self.path.parent() {
      fs::create_dir_all(parent)?;
    }
    let mut out = String::new();
    for (k, v) in &self.entries {
      out.push_str(&escape(k));
      out.push('\n');
      out.push_str(&escape(v));
      out.push('\n');
    }
    fs::write(&self.path, out)
  }
}

/// Same shape as [`LocalStorage`] - see the module docs for why this
/// doesn't actually implement `sessionStorage`'s clear-on-exit lifetime.
pub type SessionStorage = LocalStorage;
