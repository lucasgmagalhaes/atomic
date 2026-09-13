//! `Cursor` — split out from `indexed_db.rs`.

use crate::value::Value;

use super::store::ObjectStore;

/// Real ordered iteration over an [`ObjectStore`]'s keys within a
/// [`super::KeyRange`]. Mirrors `IDBCursor`'s "starts before the first entry"
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
    pub(super) fn new(store: &'a ObjectStore, keys: Vec<String>) -> Self {
        Cursor {
            store,
            keys,
            position: 0,
            started: false,
        }
    }

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
