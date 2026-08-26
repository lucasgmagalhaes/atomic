//! A scoped-down IndexedDB: multiple named object stores per database,
//! each a real flat key→value map persisted to its own file (built
//! directly on [`crate::LocalStorage`] — same file format, same
//! one-mutation-one-flush persistence), plus one secondary index per store
//! mapping an index value to the set of primary keys that share it.
//! Values are real structured-clone-shaped trees ([`crate::value::Value`])
//! now, not opaque strings — see that module's doc for what's cut from the
//! real Structured Clone Algorithm. Cursors ([`Cursor`]) give real ordered
//! iteration over a store's keys, optionally bounded by a [`KeyRange`].
//!
//! Deviations from the real spec: no versioned schema / `onupgradeneeded`,
//! no asynchronous transactions (every call here is synchronous and
//! immediately durable — there's no "transaction not yet committed"
//! state), index values must be supplied explicitly by the caller
//! ([`ObjectStore::put_indexed`]) rather than derived automatically from a
//! `keyPath` walk over the stored value (a real `keyPath`-driven index
//! would be a natural next step now that values are structured, but isn't
//! implemented here), and cursors only iterate a store's *primary* keys —
//! not an index's (real `IDBIndex.openCursor()` yields `(indexKey,
//! primaryKey, value)` triples ordered by index key; [`ObjectStore::get_by_index`]
//! remains the eager, non-cursor way to query an index).
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::value::Value;
use crate::LocalStorage;

const UNIT_SEPARATOR: char = '\u{1f}';

/// One database: a directory holding one file per object store (plus one
/// per index, `<store>.<index>.idx`).
pub struct IndexedDb {
  dir: PathBuf,
  stores: HashMap<String, ObjectStore>,
}

pub struct ObjectStore {
  data: LocalStorage,
  indexes: HashMap<String, LocalStorage>,
}

impl IndexedDb {
  /// Opens (rehydrating any stores/indexes already on disk under `dir`)
  /// the database at `dir`. Store files are named `<store>.store`,
  /// index files `<store>.<index>.idx` — both scanned back into memory
  /// on open so `object_store_names()` reflects what's actually
  /// persisted, not just what's been touched this session.
  pub fn open(dir: impl AsRef<Path>) -> io::Result<Self> {
    let dir = dir.as_ref().to_path_buf();
    let mut stores: HashMap<String, ObjectStore> = HashMap::new();

    if dir.exists() {
      for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if let Some(store_name) = file_name.strip_suffix(".store") {
          let data = LocalStorage::open(dir.join(file_name.as_ref()))?;
          stores.insert(
            store_name.to_string(),
            ObjectStore {
              data,
              indexes: HashMap::new(),
            },
          );
        }
      }
      for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if let Some(rest) = file_name.strip_suffix(".idx") {
          if let Some((store_name, index_name)) = rest.split_once('.') {
            if let Some(store) = stores.get_mut(store_name) {
              store.indexes.insert(
                index_name.to_string(),
                LocalStorage::open(dir.join(file_name.as_ref()))?,
              );
            }
          }
        }
      }
    }

    Ok(IndexedDb { dir, stores })
  }

  /// Creates the object store if it doesn't already exist (idempotent —
  /// matches `IDBDatabase.createObjectStore` being a no-op-ish "get or
  /// create" from a caller's perspective in this simplified model, since
  /// there's no version-upgrade transaction gating it here).
  pub fn create_object_store(&mut self, name: &str) -> io::Result<&mut ObjectStore> {
    if !self.stores.contains_key(name) {
      let data = LocalStorage::open(self.store_path(name))?;
      self.stores.insert(
        name.to_string(),
        ObjectStore {
          data,
          indexes: HashMap::new(),
        },
      );
    }
    Ok(self.stores.get_mut(name).unwrap())
  }

  pub fn object_store_names(&self) -> impl Iterator<Item = &str> {
    self.stores.keys().map(String::as_str)
  }

  pub fn store(&self, name: &str) -> Option<&ObjectStore> {
    self.stores.get(name)
  }

  pub fn store_mut(&mut self, name: &str) -> Option<&mut ObjectStore> {
    self.stores.get_mut(name)
  }

  /// Removes the object store and its files from disk entirely.
  pub fn delete_object_store(&mut self, name: &str) -> io::Result<()> {
    if let Some(store) = self.stores.remove(name) {
      let _ = std::fs::remove_file(self.store_path(name));
      for index_name in store.indexes.keys() {
        let _ = std::fs::remove_file(self.index_path(name, index_name));
      }
    }
    Ok(())
  }

  /// Convenience wrapper over [`ObjectStore::create_index`] that
  /// computes the index's on-disk path so callers don't need to know
  /// this database's file layout. No-op if `store_name` doesn't exist.
  pub fn create_index(&mut self, store_name: &str, index_name: &str) -> io::Result<()> {
    let path = self.index_path(store_name, index_name);
    if let Some(store) = self.stores.get_mut(store_name) {
      store.create_index(index_name, path)?;
    }
    Ok(())
  }

  fn store_path(&self, name: &str) -> PathBuf {
    self.dir.join(format!("{name}.store"))
  }

  fn index_path(&self, store: &str, index: &str) -> PathBuf {
    self.dir.join(format!("{store}.{index}.idx"))
  }
}

impl ObjectStore {
  /// Structured-clones the stored value back out (a real `Value::parse`
  /// of what was persisted, not a borrow of it — matches `IDBObjectStore.get`
  /// always handing the caller an independent copy, never a live
  /// reference into storage). `None` if the key doesn't exist, or if
  /// what's on disk somehow isn't valid wire format (shouldn't happen
  /// for anything written through `put`/`put_indexed`, but a corrupted
  /// file shouldn't panic a caller either).
  pub fn get(&self, key: &str) -> Option<Value> {
    Value::parse(self.data.get(key)?).ok()
  }

  pub fn put(&mut self, key: &str, value: impl Into<Value>) -> io::Result<()> {
    self.data.set(key, &value.into().to_wire())
  }

  pub fn delete(&mut self, key: &str) -> io::Result<()> {
    self.data.remove(key)
  }

  pub fn clear(&mut self) -> io::Result<()> {
    self.data.clear()
  }

  pub fn keys(&self) -> impl Iterator<Item = &str> {
    self.data.keys()
  }

  pub fn len(&self) -> usize {
    self.data.len()
  }

  pub fn is_empty(&self) -> bool {
    self.data.is_empty()
  }

  /// Registers an index by name (idempotent), backed by its own
  /// persisted file. Doesn't populate it from existing entries — indexes
  /// only see keys put via [`put_indexed`](Self::put_indexed) from here
  /// on, since (per the module doc) there's no `keyPath` to derive an
  /// index entry from automatically yet.
  pub fn create_index(&mut self, name: &str, path: impl AsRef<Path>) -> io::Result<()> {
    if !self.indexes.contains_key(name) {
      self
        .indexes
        .insert(name.to_string(), LocalStorage::open(path)?);
    }
    Ok(())
  }

  /// Like [`put`](Self::put), but also records `index_value` under
  /// `index_name` (which must already exist via
  /// [`create_index`](Self::create_index)) so [`get_by_index`](Self::get_by_index)
  /// can find `key` by it later. Multiple keys may share the same
  /// `index_value` (a non-unique index) — stored as a unit-separator-
  /// joined list under one entry, same trick `cookies` doesn't need but
  /// this does since `LocalStorage`'s value slot is a single string.
  pub fn put_indexed(
    &mut self,
    index_name: &str,
    key: &str,
    value: impl Into<Value>,
    index_value: &str,
  ) -> io::Result<()> {
    self.data.set(key, &value.into().to_wire())?;
    let Some(index) = self.indexes.get_mut(index_name) else {
      return Ok(());
    };
    let mut keys: Vec<String> = index
      .get(index_value)
      .map(|existing| existing.split(UNIT_SEPARATOR).map(str::to_string).collect())
      .unwrap_or_default();
    if !keys.iter().any(|k| k == key) {
      keys.push(key.to_string());
    }
    index.set(index_value, &keys.join(&UNIT_SEPARATOR.to_string()))
  }

  /// Primary keys stored under `index_value` in `index_name`, looked up
  /// through to their current (structured-cloned) values. Empty if the
  /// index or value isn't known, or if entries were since deleted via
  /// [`delete`](Self::delete) (a deletion doesn't retroactively clean
  /// the index — same "index can point at nothing" characteristic real
  /// IndexedDB avoids via engine-internal bookkeeping this simplified
  /// model doesn't replicate).
  pub fn get_by_index(&self, index_name: &str, index_value: &str) -> Vec<(&str, Value)> {
    let Some(index) = self.indexes.get(index_name) else {
      return Vec::new();
    };
    let Some(keys) = index.get(index_value) else {
      return Vec::new();
    };
    keys
      .split(UNIT_SEPARATOR)
      .filter_map(|k| {
        self
          .data
          .get(k)
          .and_then(|raw| Value::parse(raw).ok())
          .map(|v| (k, v))
      })
      .collect()
  }

  /// Opens a [`Cursor`] over this store's primary keys within `range`,
  /// in `direction` order. Real ordered iteration (keys sorted
  /// lexicographically then optionally reversed), computed eagerly at
  /// open time — a real `IDBCursor` can observe writes made *during*
  /// iteration in some engines, this one can't (its key list is a
  /// snapshot), a documented simplification rather than a hidden one.
  pub fn open_cursor(&self, range: &KeyRange, direction: CursorDirection) -> Cursor<'_> {
    let mut keys: Vec<String> = self
      .data
      .keys()
      .filter(|k| range.contains(k))
      .map(str::to_string)
      .collect();
    keys.sort();
    if direction == CursorDirection::Prev {
      keys.reverse();
    }
    Cursor {
      store: self,
      keys,
      position: 0,
      started: false,
    }
  }
}

/// Which way a [`Cursor`] walks its (already sorted) key list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorDirection {
  Next,
  Prev,
}

/// A bound on which keys a [`Cursor`] visits — mirrors `IDBKeyRange`.
/// Keys compare lexicographically (this crate's keys are always strings,
/// unlike real IndexedDB's structured-clone key comparison algorithm —
/// documented in `crate::cookies`-adjacent modules as an acceptable cut
/// for a string-keyed store).
#[derive(Debug, Clone)]
pub struct KeyRange {
  lower: Option<(String, bool)>, // (bound, exclusive)
  upper: Option<(String, bool)>,
}

impl KeyRange {
  pub fn all() -> Self {
    KeyRange {
      lower: None,
      upper: None,
    }
  }

  pub fn only(key: impl Into<String>) -> Self {
    let key = key.into();
    KeyRange {
      lower: Some((key.clone(), false)),
      upper: Some((key, false)),
    }
  }

  pub fn lower_bound(lower: impl Into<String>, open: bool) -> Self {
    KeyRange {
      lower: Some((lower.into(), open)),
      upper: None,
    }
  }

  pub fn upper_bound(upper: impl Into<String>, open: bool) -> Self {
    KeyRange {
      lower: None,
      upper: Some((upper.into(), open)),
    }
  }

  pub fn bound(
    lower: impl Into<String>,
    upper: impl Into<String>,
    lower_open: bool,
    upper_open: bool,
  ) -> Self {
    KeyRange {
      lower: Some((lower.into(), lower_open)),
      upper: Some((upper.into(), upper_open)),
    }
  }

  fn contains(&self, key: &str) -> bool {
    if let Some((lower, open)) = &self.lower {
      if *open {
        if key <= lower.as_str() {
          return false;
        }
      } else if key < lower.as_str() {
        return false;
      }
    }
    if let Some((upper, open)) = &self.upper {
      if *open {
        if key >= upper.as_str() {
          return false;
        }
      } else if key > upper.as_str() {
        return false;
      }
    }
    true
  }
}

/// Real ordered iteration over an [`ObjectStore`]'s keys within a
/// [`KeyRange`]. Mirrors `IDBCursor`'s "starts before the first entry"
/// protocol: [`advance`](Self::advance) must be called once before the
/// first [`key`](Self::key)/[`value`](Self::value) read, and again for
/// each subsequent entry - not a standard Rust `Iterator` on purpose, to
/// keep that real-API shape recognizable rather than papering over it.
pub struct Cursor<'a> {
  store: &'a ObjectStore,
  keys: Vec<String>,
  position: usize,
  started: bool,
}

impl<'a> Cursor<'a> {
  /// Moves to the next entry (the first entry, the first time this is
  /// called). Returns `false` once there are no more - matches
  /// `IDBCursor.continue()`'s "fires `success` with a `null` cursor at
  /// the end" signal, just as a return value instead of a callback.
  pub fn advance(&mut self) -> bool {
    if !self.started {
      self.started = true;
    } else {
      self.position += 1;
    }
    self.position < self.keys.len()
  }

  /// Advances `count` entries at once (`IDBCursor.advance(count)`).
  /// `count` must be at least 1, matching the real API's own
  /// requirement (unlike `advance`/`continue`'s implicit single step).
  pub fn advance_by(&mut self, count: usize) -> bool {
    assert!(count >= 1, "Cursor::advance_by count must be at least 1");
    for _ in 0..count {
      if !self.advance() {
        return false;
      }
    }
    true
  }

  /// The current entry's key, or `None` before the first `advance()`
  /// call or past the end.
  pub fn key(&self) -> Option<&str> {
    if !self.started {
      return None;
    }
    self.keys.get(self.position).map(String::as_str)
  }

  /// The current entry's structured-cloned value.
  pub fn value(&self) -> Option<Value> {
    self.store.get(self.key()?)
  }
}
