//! A scoped-down IndexedDB: multiple named object stores per database,
//! each a real flat key→value map persisted to its own file (built
//! directly on [`crate::LocalStorage`] — same file format, same
//! one-mutation-one-flush persistence), plus one secondary index per store
//! mapping an index value to the set of primary keys that share it.
//!
//! Deviations from the real spec: no versioned schema / `onupgradeneeded`,
//! no asynchronous transactions (every call here is synchronous and
//! immediately durable — there's no "transaction not yet committed"
//! state), no cursors (just [`ObjectStore::keys`] / a direct `get`), no
//! structured clone (values are opaque strings, matching
//! `LocalStorage`/`SessionStorage`'s own scope cut — same reasoning: no
//! serde in this workspace yet), and index values must be supplied
//! explicitly by the caller ([`ObjectStore::put_indexed`]) rather than
//! derived automatically from a `keyPath` into a structured value, since
//! there's no structured value to walk.
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

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
                    stores.insert(store_name.to_string(), ObjectStore { data, indexes: HashMap::new() });
                }
            }
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                if let Some(rest) = file_name.strip_suffix(".idx") {
                    if let Some((store_name, index_name)) = rest.split_once('.') {
                        if let Some(store) = stores.get_mut(store_name) {
                            store.indexes.insert(index_name.to_string(), LocalStorage::open(dir.join(file_name.as_ref()))?);
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
            self.stores.insert(name.to_string(), ObjectStore { data, indexes: HashMap::new() });
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
    pub fn get(&self, key: &str) -> Option<&str> {
        self.data.get(key)
    }

    pub fn put(&mut self, key: &str, value: &str) -> io::Result<()> {
        self.data.set(key, value)
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
    /// on, since (per the module doc) there's no structured value to
    /// derive an index entry from automatically.
    pub fn create_index(&mut self, name: &str, path: impl AsRef<Path>) -> io::Result<()> {
        if !self.indexes.contains_key(name) {
            self.indexes.insert(name.to_string(), LocalStorage::open(path)?);
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
    pub fn put_indexed(&mut self, index_name: &str, key: &str, value: &str, index_value: &str) -> io::Result<()> {
        self.data.set(key, value)?;
        let Some(index) = self.indexes.get_mut(index_name) else {
            return Ok(());
        };
        let mut keys: Vec<String> = index.get(index_value).map(|existing| existing.split(UNIT_SEPARATOR).map(str::to_string).collect()).unwrap_or_default();
        if !keys.iter().any(|k| k == key) {
            keys.push(key.to_string());
        }
        index.set(index_value, &keys.join(&UNIT_SEPARATOR.to_string()))
    }

    /// Primary keys stored under `index_value` in `index_name`, looked up
    /// through to their current values. Empty if the index or value isn't
    /// known, or if entries were since deleted via [`delete`](Self::delete)
    /// (a deletion doesn't retroactively clean the index — same "index can
    /// point at nothing" characteristic real IndexedDB avoids via engine-
    /// internal bookkeeping this simplified model doesn't replicate).
    pub fn get_by_index(&self, index_name: &str, index_value: &str) -> Vec<(&str, &str)> {
        let Some(index) = self.indexes.get(index_name) else {
            return Vec::new();
        };
        let Some(keys) = index.get(index_value) else {
            return Vec::new();
        };
        keys.split(UNIT_SEPARATOR)
            .filter_map(|k| self.data.get(k).map(|v| (k, v)))
            .collect()
    }
}
