//! `Context` — a live QuickJS context wrapping every global this crate
//! registers, plus the optional [`host_state::HostState`] backing DOM/
//! storage-aware bindings. Split into `eval.rs` (script evaluation +
//! uncaught-error reporting), `limits.rs` (time budget/memory limit),
//! `console_nav.rs` (console-message draining, pending-navigation,
//! `beforeunload`), `viewport.rs` (scroll/viewport size/`resize`),
//! `render_state.rs` (layout rects/computed styles/CSP/Permissions-Policy),
//! and `stylesheets.rs` (`adoptedStyleSheets` version/text) — this file
//! keeps the struct itself, construction, the DOM/URL/lifecycle-event
//! accessors, and `Drop`.

use std::marker::PhantomData;
use std::path::Path;

use quickjs_sys as sys;

use crate::{EvalError, Runtime};

mod console_nav;
mod eval;
mod limits;
mod render_state;
mod stylesheets;
mod viewport;

pub struct Context<'rt> {
    pub(crate) ptr: *mut sys::JSContext,
    _runtime: PhantomData<&'rt Runtime>,
    // Kept alive here (not just handed to JS_SetContextOpaque) so it's freed
    // on Context::drop instead of leaking. Moving the Box moves this struct's
    // pointer field, not the heap allocation, so the raw pointer registered
    // with QuickJS via register() stays valid regardless.
    pub(crate) _host_state: Option<Box<host_state::HostState>>,
    /// Per-eval time budget (`script_limits`); `None` until a host calls
    /// `set_time_budget`.
    pub(crate) _time_budget: Option<std::time::Duration>,
}

use crate::{
    abort_controller, blob, clipboard, computed_style, console, crypto, cssom_stylesheet,
    custom_elements, document_cookie, dom_bindings, event_subclasses, events, fetch, fetch_async,
    form_data, history, host_state, import_map, indexed_db_bindings, local_storage_bindings,
    location, message_channel, module_loader, mutation_observer, navigator, notifications,
    page_visibility, performance, request_response, screen, script_limits, selection, timers,
    trusted_types, url_bindings, value_bridge, web_audio, window, window_registry, worker_bindings,
};

/// Registers every global this crate exposes on a fresh `JSContext` —
/// factored out of [`Context::new`] so [`window_registry`]'s
/// `window.open()` support (`ROADMAP.md` items 36/37) can build a second,
/// independent `JSContext` sharing the same `JSRuntime` without going
/// through the safe [`Context`] wrapper's own `&'rt Runtime`-borrowing
/// lifetime (a dynamically-opened window has no such borrow to hold —
/// its lifetime is managed by `window_registry`'s own registry instead).
pub(crate) unsafe fn register_standard_globals(ptr: *mut sys::JSContext) {
    performance::register(ptr);
    console::register(ptr);
    abort_controller::register(ptr);
    crypto::register(ptr);
    events::register(ptr);
    event_subclasses::register(ptr);
    page_visibility::register(ptr);
    request_response::register(ptr);
    fetch::register(ptr);
    fetch_async::register(ptr);
    timers::register(ptr);
    document_cookie::register(ptr);
    cssom_stylesheet::register(ptr);
    trusted_types::register(ptr);
    indexed_db_bindings::register(ptr);
    local_storage_bindings::register(ptr);
    blob::register(ptr);
    url_bindings::register(ptr);
    form_data::register(ptr);
    notifications::register(ptr);
    navigator::register(ptr);
    clipboard::register(ptr);
    web_audio::register(ptr);
    window::register(ptr);
    location::register(ptr);
    history::register(ptr);
    message_channel::register(ptr);
    mutation_observer::register(ptr);
    custom_elements::register(ptr);
    screen::register(ptr);
    value_bridge::register(ptr);
    selection::register(ptr);
    worker_bindings::register(ptr);
}

/// The exact inverse of [`register_standard_globals`] — every module's
/// `cleanup(ctx)` that must run before `JS_FreeContext`, minus
/// `dom_bindings::cleanup` (only relevant for a `with_dom`-flavored
/// context, called separately by whichever caller attached DOM bindings
/// in the first place).
pub(crate) unsafe fn cleanup_standard_globals(ptr: *mut sys::JSContext) {
    custom_elements::cleanup(ptr);
    script_limits::cleanup(ptr);
    timers::cleanup(ptr);
    fetch_async::cleanup(ptr);
    mutation_observer::cleanup(ptr);
    message_channel::cleanup(ptr);
    blob::cleanup(ptr);
    notifications::cleanup(ptr);
    selection::cleanup(ptr);
    worker_bindings::cleanup(ptr);
}

/// Builds a fresh, unattached [`host_state::HostState`] wrapping `dom`
/// with a brand-new [`window_registry::WindowId`] — factored out of
/// [`Context::with_dom`] for the same reason [`register_standard_globals`]
/// is: `window_registry`'s dynamically-opened windows need this without
/// constructing a safe [`Context`].
pub(crate) fn build_host_state(dom: dom::Dom) -> Box<host_state::HostState> {
    Box::new(host_state::HostState {
        dom,
        cookies: None,
        host: String::new(),
        storage_dir: None,
        local_storage: None,
        session_storage: None,
        url: None,
        layout_rects: std::collections::HashMap::new(),
        computed_styles: std::collections::HashMap::new(),
        scroll_extents: std::collections::HashMap::new(),
        csp: Vec::new(),
        trusted_type_policy_names: Vec::new(),
        console_messages: Vec::new(),
        permissions_policy: None,
        scroll_y: 0.0,
        viewport_width: 0.0,
        viewport_height: 0.0,
        pending_navigation: None,
        window: host_state::WindowState {
            id: window_registry::register(),
        },
    })
}

impl<'rt> Context<'rt> {
    pub fn new(runtime: &'rt Runtime) -> Self {
        let ptr = unsafe { sys::JS_NewContext(runtime.ptr) };
        assert!(!ptr.is_null(), "JS_NewContext returned null");
        unsafe { register_standard_globals(ptr) };
        Context {
            ptr,
            _runtime: PhantomData,
            _host_state: None,
            _time_budget: None,
        }
    }

    /// Same as [`Context::new`], but also registers the minimal DOM
    /// bindings (see `dom_bindings`) backed by `dom`. `document.cookie`
    /// and `indexedDB` are already globally present from [`Context::new`]
    /// but stay inert (empty cookie string, `indexedDB.open` returning
    /// `null`) until [`Context::with_storage`] configures real storage.
    pub fn with_dom(runtime: &'rt Runtime, dom: dom::Dom) -> Self {
        let mut ctx = Self::new(runtime);
        let mut state = build_host_state(dom);
        let raw = state.as_mut() as *mut host_state::HostState as *mut std::os::raw::c_void;
        unsafe {
            sys::JS_SetContextOpaque(ctx.ptr, raw);
            dom_bindings::register(ctx.ptr);
            computed_style::register(ctx.ptr);
        }
        ctx._host_state = Some(state);
        ctx
    }

    /// Same as [`Context::with_dom`], but also wires real persisted
    /// `document.cookie`, `indexedDB`, `localStorage`, and `sessionStorage`
    /// access: cookies live in `storage_dir/cookies.txt` (scoped to
    /// `host`, used as the default `Domain` for cookies set without one),
    /// `indexedDB.open(name)` resolves each database under
    /// `storage_dir/idb/<name>`, and `localStorage`/`sessionStorage` live
    /// in `storage_dir/local_storage.txt`/`session_storage.txt`.
    pub fn with_storage(
        runtime: &'rt Runtime,
        dom: dom::Dom,
        host: &str,
        storage_dir: impl AsRef<Path>,
    ) -> std::io::Result<Self> {
        let storage_dir = storage_dir.as_ref().to_path_buf();
        let cookies = storage::cookies::CookieJar::open(storage_dir.join("cookies.txt"))?;
        let local_storage = storage::LocalStorage::open(storage_dir.join("local_storage.txt"))?;
        let session_storage = storage::LocalStorage::open(storage_dir.join("session_storage.txt"))?;

        let mut ctx = Self::with_dom(runtime, dom);
        if let Some(state) = ctx._host_state.as_mut() {
            state.cookies = Some(cookies);
            state.host = host.to_string();
            state.storage_dir = Some(storage_dir);
            state.local_storage = Some(local_storage);
            state.session_storage = Some(session_storage);
        }
        Ok(ctx)
    }

    /// Fires every due `setTimeout`/`setInterval` and every queued
    /// `requestAnimationFrame` callback, resolves/rejects every completed
    /// `fetch()`/`XMLHttpRequest`, and drains quickjs's own Promise job
    /// queue so `.then()` continuations from any of the above actually
    /// run. See `timers`/`fetch_async`'s module docs — there's no real
    /// event loop yet, so nothing calls this on its own; the host
    /// (`profile-worker`'s per-frame loop, or tests) must pump it
    /// explicitly. Returns how many callbacks/jobs ran in total.
    pub fn run_pending_timers(&self) -> usize {
        let window_delivered = self
            .window_id()
            .map(|id| unsafe { window_registry::pump(self.ptr, id) })
            .unwrap_or(0);
        // Any window this context opened via `window.open()` (`ROADMAP.md`
        // items 36/37) has no host loop of its own driving it — piggyback
        // on this context's own pump instead, recursively, so the whole
        // tree of windows a page opens gets driven by the same per-frame
        // `run_pending_timers` call the top-level host already makes. See
        // `window_registry::pump_children`'s own doc.
        let children_delivered = unsafe { window_registry::pump_children(self.ptr) };
        unsafe {
            timers::pump(self.ptr)
                + fetch_async::pump(self.ptr)
                + mutation_observer::pump(self.ptr)
                + message_channel::pump(self.ptr)
                + window_delivered
                + module_loader::pump(self.ptr)
                + worker_bindings::pump(self.ptr)
                + children_delivered
        }
    }

    /// Resolves an `import`/dynamic-`import()` specifier against
    /// `base_url` — real relative/absolute URL resolution (`ROADMAP.md`
    /// item 19's resolver half), plus a real import map for bare
    /// specifiers (see `module_loader`/`import_map`'s own docs).
    pub fn resolve_module_specifier(&self, base_url: &str, specifier: &str) -> Option<String> {
        unsafe { module_loader::resolve_specifier(self.ptr, base_url, specifier) }
    }

    /// Real import map (`ROADMAP.md` item 19's remaining sub-item):
    /// parses `json` (`{"imports": {"bare": "url", "prefix/": "url/"}}`)
    /// and replaces this `Context`'s import map wholesale — see
    /// `import_map`'s own doc for the exact-then-longest-prefix
    /// resolution rule and scope cuts (no `scopes`, last write wins).
    /// `base_url` is the real document URL a root-relative mapped value
    /// (e.g. `"/src/app/"`) resolves against, same real spec rule a
    /// `<script type="importmap">`'s own values follow.
    pub fn set_import_map(&self, base_url: &str, json: &str) -> Result<(), String> {
        unsafe { import_map::set_import_map(self.ptr, base_url, json) }
    }

    /// Starts (or reuses, if already cached/in-flight) a background fetch
    /// of `resolved_url`'s source text — see `module_loader`'s own doc.
    /// Delivered the next time [`Context::run_pending_timers`] pumps.
    pub fn load_module(&self, resolved_url: &str) {
        module_loader::load(self.ptr, resolved_url);
    }

    /// Real module-fetch cache read side — `None` if [`Context::
    /// load_module`] was never called for `resolved_url` on this
    /// `Context`.
    pub fn module_status(&self, resolved_url: &str) -> Option<module_loader::ModuleStatus> {
        module_loader::status(self.ptr, resolved_url)
    }

    /// This context's window identity (`ROADMAP.md` items 35-37) — `None`
    /// for a plain [`Context::new`]/`with_dom` predates window registration
    /// only in the sense that no host state exists at all; every
    /// [`Context::with_dom`] context gets one. See `window_registry`'s
    /// own module doc for why this exists ahead of `iframe`/`window.open`
    /// themselves.
    pub fn window_id(&self) -> Option<window_registry::WindowId> {
        self._host_state.as_ref().map(|s| s.window.id)
    }

    /// Evaluates `code` in a window this context (or any context)
    /// opened via `window.open()` — there is no JS-facing way to reach
    /// another window's realm directly (by design, matching real
    /// cross-window isolation), so this is a host/test-facing debugging
    /// hook, not something a page script can call. `None` if `id` isn't
    /// a currently-open, dynamically-opened window (an externally-owned
    /// top-level `Context`'s own id doesn't resolve here either — this
    /// only reaches windows `window_registry` itself owns).
    pub fn eval_in_window(
        &self,
        id: window_registry::WindowId,
        code: &str,
        filename: &str,
    ) -> Option<Result<String, EvalError>> {
        let ptr = window_registry::context_ptr_for(id)?;
        Some(unsafe { Self::eval_raw(ptr, code, filename) })
    }

    /// Rust-level `window.open()` — opens a real child window the same
    /// way the JS-facing `window.open()` global does, without going
    /// through `eval`. A host/test-facing convenience for driving the
    /// primitive directly (same "Rust-level entry point alongside the
    /// JS one" shape [`Context::send_cross_window_message`] already has).
    pub fn open_window(&self) -> window_registry::WindowId {
        unsafe { window_registry::open_window(self.ptr) }
    }

    /// The child window an `<iframe>` (`id`) exposes via `.contentWindow`
    /// (`ROADMAP.md` item 35) — opens one on first call, same as the
    /// JS-facing getter, and returns the same `WindowId` on every later
    /// call for the same `id`. A host/test-facing Rust-level entry point
    /// alongside the JS one, same shape [`Context::open_window`] already
    /// has for `window.open()`.
    pub fn window_id_for_iframe(&self, id: dom::NodeId) -> window_registry::WindowId {
        unsafe { window_registry::window_for_iframe(self.ptr, id) }
    }

    /// Sends `js_expr`'s evaluated result (cloned via the same
    /// context-independent `storage::value::Value` bridge `structuredClone`
    /// uses) to window `to`'s inbox, delivered on that window's own
    /// `Context` the next time its `run_pending_timers` pumps. Returns
    /// `false` if `js_expr` throws or `to` isn't a live window. This is a
    /// Rust-level primitive for now, not a JS-facing `postMessage` — see
    /// `window_registry`'s own doc for the scope cut.
    pub fn send_cross_window_message(&self, to: window_registry::WindowId, js_expr: &str) -> bool {
        // Evaluates the raw JSValue directly (not via `Context::eval`,
        // which stringifies the result) since the value itself, not its
        // string form, is what needs to cross into `to`'s inbox.
        unsafe {
            let c_src = std::ffi::CString::new(js_expr).unwrap_or_default();
            let c_name = std::ffi::CString::new("<cross-window-message>").unwrap();
            let value = sys::JS_Eval(
                self.ptr,
                c_src.as_ptr(),
                js_expr.len(),
                c_name.as_ptr(),
                sys::JS_EVAL_TYPE_GLOBAL as i32,
            );
            if sys::js_is_exception(&value) {
                sys::JS_FreeValue(self.ptr, value);
                return false;
            }
            let sent = window_registry::send(self.ptr, to, value);
            sys::JS_FreeValue(self.ptr, value);
            sent
        }
    }

    /// The DOM this context was constructed with via
    /// [`Context::with_dom`], or `None` for a plain [`Context::new`]. Lets
    /// a host that mutated the DOM through JS (e.g. a `setInterval`
    /// callback calling `textContent = ...`) read it back to re-run
    /// layout/render against the live state — see `profile-worker`'s
    /// per-frame loop.
    pub fn dom(&self) -> Option<&dom::Dom> {
        self._host_state.as_ref().map(|s| &s.dom)
    }

    /// Mutable counterpart to [`Context::dom`] — lets a host mutate the DOM
    /// directly from Rust (not through `eval`). Not currently used by
    /// `profile-worker`'s own focus handling: that goes through the real
    /// `.focus()`/`.blur()` JS bindings instead (`focus_element`/
    /// `blur_element` in `profile_worker.rs`), so the real `"focus"`/
    /// `"blur"`/`"change"` events dispatch too, which a direct
    /// `dom::Dom::focus`/`blur` call bypasses. Kept for a host that
    /// genuinely needs to mutate DOM state without touching JS at all.
    pub fn dom_mut(&mut self) -> Option<&mut dom::Dom> {
        self._host_state.as_mut().map(|s| &mut s.dom)
    }

    /// Sets the page's real URL, backing `location`'s reflected properties
    /// (see `location`). No-op on a plain [`Context::new`] (nothing to set
    /// it on) — a caller with a real navigated URL (`profile-worker`'s
    /// `base_url`) calls this once after construction; `history.rs`'s
    /// `pushState`/`replaceState`/`back`/`forward`/`go` update it further
    /// for same-document navigation.
    pub fn set_url(&mut self, url: &str) {
        if let Some(state) = self._host_state.as_mut() {
            state.url = Some(url.to_string());
        }
    }

    /// Fires real `DOMContentLoaded` (on `document`, bubbles) and `load`
    /// (on `window`/the global object, doesn't bubble) — the two lifecycle
    /// events this crate models so far (see `JS_ENGINE_CAPABILITY_MATRIX.md`
    /// item 5). A host with a real parsed/scripted page
    /// (`profile-worker`'s `Page::load`, after running every page script)
    /// calls this once per navigation/reload, matching the real DOM's
    /// "fires once the document is parsed and its scripts have run" timing
    /// — this crate has no separate parser-async/subresource-loading
    /// timeline to distinguish `DOMContentLoaded` from `load` further than
    /// that, a documented scope cut (both currently fire back to back).
    pub fn dispatch_lifecycle_events(&self) {
        unsafe {
            let document = crate::document::get_or_create(self.ptr);
            events::dispatch_simple(self.ptr, document, "DOMContentLoaded", true, false);
            sys::JS_FreeValue(self.ptr, document);
            let global = sys::JS_GetGlobalObject(self.ptr);
            events::dispatch_simple(self.ptr, global, "load", false, false);
            sys::JS_FreeValue(self.ptr, global);
        }
    }

    /// Raw `JSContext` pointer, for a caller outside this crate that needs
    /// to register its own native globals (e.g. `automation`'s `pane`/
    /// `every`/`on`/cron bindings) via `quickjs-sys` directly, the same way
    /// every `src/*_bindings.rs` module in this crate already does. Kept
    /// deliberately narrow — callers get the pointer, not a way to install
    /// a hook `Context::new` itself calls, since only one caller
    /// (`automation`) needs this so far and a generic registration API
    /// would be speculative.
    pub fn as_raw(&self) -> *mut sys::JSContext {
        self.ptr
    }
}

impl Drop for Context<'_> {
    fn drop(&mut self) {
        unsafe { window_registry::close_children_of(self.ptr) };
        unsafe { window_registry::cleanup_remote_window_cache(self.ptr) };
        if let Some(id) = self.window_id() {
            window_registry::unregister(id);
        }
        module_loader::cleanup(self.ptr);
        import_map::cleanup(self.ptr);
        unsafe {
            cleanup_standard_globals(self.ptr);
            dom_bindings::cleanup(self.ptr);
            sys::JS_FreeContext(self.ptr);
        }
    }
}
