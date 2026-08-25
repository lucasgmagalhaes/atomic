//! What lives behind a `Context`'s `JS_SetContextOpaque` slot: the DOM
//! (`dom_bindings`, `document_cookie`, `indexed_db_bindings` all read/
//! mutate through this one struct's raw pointer, converging on one shape
//! regardless of which `Context` constructor built it) plus, once real
//! persisted storage is configured via `Context::with_storage`, a cookie
//! jar and the directory `indexedDB.open` resolves per-database paths
//! against.
use std::collections::HashMap;
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
    /// Real layout results, pushed in wholesale by `Context::
    /// set_layout_rects` (see `crate::layout_measurement`) after a host
    /// runs `layout-engine` against the current DOM. A node absent here
    /// (never laid out, `display: none`, or nothing has called
    /// `set_layout_rects` at all) reads as an all-zero rect, same
    /// degrade-gracefully pattern as `url`/`cookies`.
    pub layout_rects: HashMap<dom::NodeId, crate::layout_measurement::Rect>,
    /// Real cascaded property values, pushed in wholesale by `Context::
    /// set_computed_styles` (see `crate::computed_style`) after a host runs
    /// `layout-engine` against the current DOM — same "host computes it,
    /// context just stores the result" shape as `layout_rects`. A node
    /// absent here (never laid out, or nothing has called
    /// `set_computed_styles`) reads as an empty style, same
    /// degrade-gracefully pattern as `layout_rects`/`url`.
    pub computed_styles: HashMap<dom::NodeId, HashMap<String, String>>,
    /// Raw `Content-Security-Policy` policy texts, each checked by
    /// `crate::csp::is_connect_allowed` before `fetch`/`fetchSync`/
    /// `XMLHttpRequest` send a request — a request must be allowed by
    /// *every* entry (real CSP's multiple-policy model: several delivered
    /// policies — repeated response headers, `<meta http-equiv>` tags —
    /// intersect, they don't merge). Empty (a plain `Context::with_dom`,
    /// or a host that never calls `Context::set_csp`/`add_csp_policy`)
    /// enforces nothing — same degrade-gracefully pattern as
    /// `url`/`layout_rects`.
    pub csp: Vec<String>,
    /// Every `trustedTypes.createPolicy` name already created in this
    /// context — real Trusted Types policy names must be unique per
    /// document, so a duplicate creation is rejected. Empty on a bare
    /// `Context::new` (no host state behind the sink checks), where the
    /// uniqueness check degrades to accepting duplicates.
    pub trusted_type_policy_names: Vec<String>,
    /// Raw `Permissions-Policy` response-policy text. Native capability
    /// bindings consult it immediately before performing their privileged
    /// operation; `None` means the host did not provide a policy and leaves
    /// the feature available.
    pub permissions_policy: Option<String>,
}

/// Reads the `HostState` behind `ctx`'s opaque slot, or null if unset
/// (a plain `Context::new`/`with_dom`-without-storage, or nothing set
/// yet).
pub(crate) unsafe fn get(ctx: *mut sys::JSContext) -> *mut HostState {
    sys::JS_GetContextOpaque(ctx) as *mut HostState
}
