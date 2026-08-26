//! `FormData` — an ordered name→value list backed by a real `Vec<Entry>`
//! (not a placeholder object), with the full per-spec method surface:
//! `append`/`delete`/`get`/`getAll`/`has`/`set`/`forEach`, plus
//! `entries()`/`keys()`/`values()`.
//!
//! Split into one file per cohesive responsibility group (SRP), same
//! convention `dom_bindings`'s own split uses:
//! - [`util`]: shared string/error helpers and the class-kind name.
//! - [`types`]: the stored representation (`FormDataInner`/`Entry`/
//!   `EntryValue`), its size bounds, and the opaque-data plumbing.
//! - [`encoding`]: `multipart/form-data` serialization (`serialize`, used
//!   by `fetch`/`XMLHttpRequest`) and the stored-entry-to-JS-value
//!   direction (`entry_to_js`, `read_entry_value`, `blob_from_bytes`).
//! - [`form_populate`]: `new FormData(form)`'s control-walking population.
//! - [`class`]: the native constructor.
//! - [`methods`]: `FormData.prototype`'s method surface.
//! - [`register`]: wires the class/prototype/constructor into the global.
//!
//! Deviations from the real API (each mirrors an existing documented cut
//! elsewhere in this crate):
//!
//! * `new FormData(form)` populates from the passed `<form>`'s controls —
//!   named `input`s (`checkbox`/`radio` only when checked, everything else
//!   via the generic `.value`, so a `<textarea>` falls back to its text
//!   content exactly like `.value` does) and named `<select>`s with an
//!   explicitly-selected `<option>` (effective value = its `value`
//!   attribute or text content). Buttons are never included (real spec
//!   includes only the submitter, which doesn't exist in this engine), and
//!   file inputs degrade to their `value` attribute (no filesystem).
//!   Without a live DOM behind the context (plain [`Context::new`]) the
//!   constructor degrades to an empty instance rather than throwing.
//! * Stored file values are snapshots: appending a `Blob`/`File` copies its
//!   bytes immediately (the real spec does this too); `get()` returns a
//!   fresh genuine `Blob` instance per call carrying `name`/`lastModified`
//!   own properties when a filename was given.
//! * `entries()`/`keys()`/`values()` return plain arrays of pairs/values,
//!   not iterator objects — QuickJS's well-known-symbol lookup isn't bound
//!   in `quickjs-sys` yet, same reason `attributes` iteration uses arrays.
//!   Use `forEach` for callback-style walking.

mod class;
mod encoding;
mod form_populate;
mod methods;
mod register;
mod types;
mod util;

pub(crate) use encoding::serialize;
pub(crate) use register::register;
