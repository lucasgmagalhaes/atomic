//! `IndexedDb` — split out from `indexed_db.rs`.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::LocalStorage;

use super::store::ObjectStore;

/// One database: a directory holding one file per object store (plus one
/// per index, `<store>.<index>.idx`).
pub struct IndexedDb {
    dir: PathBuf,
    stores: HashMap<String, ObjectStore>,
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
                    stores.insert(store_name.to_string(), ObjectStore::new(data));
                }
            }
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                if let Some(rest) = file_name.strip_suffix(".idx") {
                    if let Some((store_name, index_name)) = rest.split_once('.') {
                        if let Some(store) = stores.get_mut(store_name) {
                            store.insert_index(
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
            self.stores.insert(name.to_string(), ObjectStore::new(data));
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
            for index_name in store.index_names() {
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
