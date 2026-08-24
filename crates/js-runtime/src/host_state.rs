//! What lives behind a `Context`'s `JS_SetContextOpaque` slot: the DOM
//! (`dom_bindings`, `document_cookie`, `indexed_db_bindings` all read/
//! mutate through this one struct's raw pointer, converging on one shape
//! regardless of which `Context` constructor built it) plus, once real
//! persisted storage is configured via `Context::with_storage`, a cookie
//! jar and the directory `indexedDB.open` resolves per-database paths
//! against.
use std::path::PathBuf;

use quickjs_sys as sys;

pub(crate) struct HostState {
    pub dom: dom::Dom,
    /// `None` for a `Context::with_dom` built without storage - the
    /// `document.cookie` accessor degrades to reading/writing nothing
    /// rather than panicking (matches this workspace's general pattern of
    /// null-checked opaque pointers rather than assuming a fully wired
    /// context).
    pub cookies: Option<storage::cookies::CookieJar>,
    /// Used as the default `Domain` for cookies set without one, and (once
    /// URL/navigation tracking exists - it doesn't yet) would scope which
    /// cookies a request actually sees. Empty string for a plain
    /// `Context::with_dom`.
    pub host: String,
    /// Base directory `indexedDB.open(name)` resolves `<dir>/idb/<name>`
    /// against. `None` disables `indexedDB` (`open` returns `null`).
    pub storage_dir: Option<PathBuf>,
    /// Backs the `localStorage`/`sessionStorage` globals (`local_storage_bindings`).
    /// `None` (a plain `Context::with_dom`) makes both read as empty and
    /// silently drop writes, same degrade-gracefully pattern as `cookies`.
    /// Real `sessionStorage`'s clear-on-tab-close lifetime isn't modeled —
    /// see `storage::LocalStorage`'s own doc on why `SessionStorage` is
    /// just an alias persisted the same way.
    pub local_storage: Option<storage::LocalStorage>,
    pub session_storage: Option<storage::LocalStorage>,
    /// The page's own URL, backing `location`'s reflected properties (see
    /// `crate::location`). `None` for a plain `Context::with_dom`/a page
    /// with no real navigated URL (e.g. `profile-worker`'s built-in demo
    /// page) — `location` degrades to empty strings rather than panicking,
    /// same pattern `cookies`/`local_storage` already use. Set via
    /// `Context::set_url`; also updated in place by `history.pushState`/
    /// `replaceState`/`back`/`forward`/`go` (same-document navigation, per
    /// spec, doesn't need a real fetch).
    pub url: Option<String>,
}

/// Reads the `HostState` behind `ctx`'s opaque slot, or null if unset
/// (a plain `Context::new`/`with_dom`-without-storage, or nothing set
/// yet).
pub(crate) unsafe fn get(ctx: *mut sys::JSContext) -> *mut HostState {
    sys::JS_GetContextOpaque(ctx) as *mut HostState
}
