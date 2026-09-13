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
//!
//! Split into `db.rs` (`IndexedDb`), `store.rs` (`ObjectStore`),
//! `range.rs` (`CursorDirection`/`KeyRange`), and `cursor.rs` (`Cursor`).

mod cursor;
mod db;
mod range;
mod store;

pub use cursor::Cursor;
pub use db::IndexedDb;
pub use range::{CursorDirection, KeyRange};
pub use store::ObjectStore;
