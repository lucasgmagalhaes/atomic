//! `ObjectStore` — split out from `indexed_db.rs`.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use crate::value::Value;
use crate::LocalStorage;

use super::cursor::Cursor;
use super::range::{CursorDirection, KeyRange};

const UNIT_SEPARATOR: char = '\u{1f}';

pub struct ObjectStore {
    data: LocalStorage,
    indexes: HashMap<String, LocalStorage>,
}

impl ObjectStore {
    pub(super) fn new(data: LocalStorage) -> Self {
        ObjectStore {
            data,
            indexes: HashMap::new(),
        }
    }

    pub(super) fn insert_index(&mut self, name: String, storage: LocalStorage) {
        self.indexes.insert(name, storage);
    }

    pub(super) fn index_names(&self) -> impl Iterator<Item = &str> {
        self.indexes.keys().map(String::as_str)
    }

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
            self.indexes
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
        keys.split(UNIT_SEPARATOR)
            .filter_map(|k| {
                self.data
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
        Cursor::new(self, keys)
    }
}
