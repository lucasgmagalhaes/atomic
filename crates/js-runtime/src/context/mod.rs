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

use crate::Runtime;

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
    document_cookie, dom_bindings, event_subclasses, events, fetch, fetch_async, form_data,
    history, host_state, indexed_db_bindings, local_storage_bindings, location, message_channel,
    mutation_observer, navigator, notifications, page_visibility, performance, request_response,
    screen, script_limits, timers, trusted_types, url_bindings, value_bridge, web_audio, window,
};

impl<'rt> Context<'rt> {
    pub fn new(runtime: &'rt Runtime) -> Self {
        let ptr = unsafe { sys::JS_NewContext(runtime.ptr) };
        assert!(!ptr.is_null(), "JS_NewContext returned null");
        unsafe {
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
            screen::register(ptr);
            value_bridge::register(ptr);
        };
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
        let mut state = Box::new(host_state::HostState {
            dom,
            cookies: None,
            host: String::new(),
            storage_dir: None,
            local_storage: None,
            session_storage: None,
            url: None,
            layout_rects: std::collections::HashMap::new(),
            computed_styles: std::collections::HashMap::new(),
            csp: Vec::new(),
            trusted_type_policy_names: Vec::new(),
            console_messages: Vec::new(),
            permissions_policy: None,
            scroll_y: 0.0,
            viewport_width: 0.0,
            viewport_height: 0.0,
            pending_navigation: None,
        });
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
        unsafe {
            timers::pump(self.ptr)
                + fetch_async::pump(self.ptr)
                + mutation_observer::pump(self.ptr)
                + message_channel::pump(self.ptr)
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
        unsafe {
            script_limits::cleanup(self.ptr);
            timers::cleanup(self.ptr);
            fetch_async::cleanup(self.ptr);
            mutation_observer::cleanup(self.ptr);
            message_channel::cleanup(self.ptr);
            blob::cleanup(self.ptr);
            notifications::cleanup(self.ptr);
            dom_bindings::cleanup(self.ptr);
            sys::JS_FreeContext(self.ptr);
        }
    }
}
