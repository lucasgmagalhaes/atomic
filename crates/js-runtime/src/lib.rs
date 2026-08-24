//! Safe wrapper around `quickjs-sys`. Minimal on purpose (phase 2 slice):
//! create a runtime/context and eval JS to a string. DOM↔JS bindings land
//! in a later pass once `dom` has something worth exposing.

use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::path::Path;

use quickjs_sys as sys;

mod blob;
mod class_registry;
mod clipboard;
mod computed_style;
mod crypto;
mod cors;
mod csp;
mod css_style;
mod cssom_stylesheet;
mod document;
mod document_cookie;
mod dom_bindings;
mod events;
mod event_subclasses;
mod fetch;
mod fetch_async;
mod history;
mod host_state;
mod indexed_db_bindings;
mod layout_measurement;
mod local_storage_bindings;
mod location;
mod notifications;
mod page_visibility;
mod performance;
mod timers;
mod value_bridge;
mod web_audio;
mod window;

pub use layout_measurement::Rect;

#[derive(Debug)]
pub struct EvalError(pub String);

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JS exception: {}", self.0)
    }
}

impl std::error::Error for EvalError {}

pub struct Runtime {
    ptr: *mut sys::JSRuntime,
}

impl Runtime {
    pub fn new() -> Self {
        let ptr = unsafe { sys::JS_NewRuntime() };
        assert!(!ptr.is_null(), "JS_NewRuntime returned null");
        Runtime { ptr }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // Must run before JS_FreeRuntime: once this runtime's memory can
        // be reused for a new one, a stale class_registry entry at the
        // same address would corrupt that new runtime's class
        // registration (see class_registry's module docs).
        class_registry::cleanup_runtime(self.ptr);
        unsafe { sys::JS_FreeRuntime(self.ptr) };
    }
}

// The runtime owns no thread-local state we rely on here, but QuickJS
// runtimes are not meant to be touched from multiple threads at once.
// Leave Runtime !Send/!Sync (the default for a raw-pointer field) until
// the profile/ipc process-per-tab model defines how this actually gets
// used across threads.

pub struct Context<'rt> {
    ptr: *mut sys::JSContext,
    _runtime: PhantomData<&'rt Runtime>,
    // Kept alive here (not just handed to JS_SetContextOpaque) so it's freed
    // on Context::drop instead of leaking. Moving the Box moves this struct's
    // pointer field, not the heap allocation, so the raw pointer registered
    // with QuickJS via register() stays valid regardless.
    _host_state: Option<Box<host_state::HostState>>,
}

impl<'rt> Context<'rt> {
    pub fn new(runtime: &'rt Runtime) -> Self {
        let ptr = unsafe { sys::JS_NewContext(runtime.ptr) };
        assert!(!ptr.is_null(), "JS_NewContext returned null");
        unsafe {
            performance::register(ptr);
            crypto::register(ptr);
            events::register(ptr);
            event_subclasses::register(ptr);
            page_visibility::register(ptr);
            fetch::register(ptr);
            fetch_async::register(ptr);
            timers::register(ptr);
            document_cookie::register(ptr);
            cssom_stylesheet::register(ptr);
            indexed_db_bindings::register(ptr);
            local_storage_bindings::register(ptr);
            blob::register(ptr);
            notifications::register(ptr);
            clipboard::register(ptr);
            web_audio::register(ptr);
            window::register(ptr);
            location::register(ptr);
            history::register(ptr);
        };
        Context {
            ptr,
            _runtime: PhantomData,
            _host_state: None,
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
            csp: None,
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
    pub fn with_storage(runtime: &'rt Runtime, dom: dom::Dom, host: &str, storage_dir: impl AsRef<Path>) -> std::io::Result<Self> {
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

    /// Evaluates `code` as global script and returns the result coerced to
    /// a string, mirroring `JS_ToCString`. On a JS-level exception, returns
    /// `Err` with a placeholder message — hooking up `JS_GetException` for
    /// a real error value/stack is follow-up work, not yet bound.
    pub fn eval(&self, code: &str, filename: &str) -> Result<String, EvalError> {
        let code_c = CString::new(code).expect("script source must not contain NUL bytes");
        let filename_c =
            CString::new(filename).expect("filename must not contain NUL bytes");

        let result = unsafe {
            sys::JS_Eval(
                self.ptr,
                code_c.as_ptr(),
                code_c.as_bytes().len(),
                filename_c.as_ptr(),
                sys::JS_EVAL_TYPE_GLOBAL,
            )
        };

        if sys::js_is_exception(&result) {
            unsafe { sys::JS_FreeValue(self.ptr, result) };
            return Err(EvalError("script raised an exception".to_string()));
        }

        let mut len: usize = 0;
        let c_str_ptr =
            unsafe { sys::JS_ToCStringLen2(self.ptr, &mut len, result, false) };
        unsafe { sys::JS_FreeValue(self.ptr, result) };

        if c_str_ptr.is_null() {
            return Err(EvalError("failed to stringify result".to_string()));
        }

        let owned = unsafe { CStr::from_ptr(c_str_ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { sys::JS_FreeCString(self.ptr, c_str_ptr) };

        Ok(owned)
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
        unsafe { timers::pump(self.ptr) + fetch_async::pump(self.ptr) }
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

    /// Replaces every real layout rect (`getBoundingClientRect`/
    /// `offsetWidth`/etc — see `layout_measurement`) wholesale — a host
    /// (`profile-worker`'s `Page::render`, after it runs `layout-engine`
    /// against the current DOM) calls this once per render pass. No-op on
    /// a plain [`Context::new`], same as [`Context::set_url`].
    pub fn set_layout_rects(&mut self, rects: std::collections::HashMap<dom::NodeId, Rect>) {
        if let Some(state) = self._host_state.as_mut() {
            state.layout_rects = rects;
        }
    }

    /// Replaces every real cascaded computed style (`getComputedStyle` —
    /// see `computed_style`) wholesale — a host (`profile-worker`'s
    /// `Page::render`, after it runs `layout-engine`'s cascade resolver
    /// against the current DOM) calls this once per render pass, same
    /// shape as [`Context::set_layout_rects`]. No-op on a plain
    /// [`Context::new`]/`with_dom` that never calls it.
    pub fn set_computed_styles(&mut self, styles: std::collections::HashMap<dom::NodeId, std::collections::HashMap<String, String>>) {
        if let Some(state) = self._host_state.as_mut() {
            state.computed_styles = styles;
        }
    }

    /// Sets the real `Content-Security-Policy` text `fetch`/`fetchSync`/
    /// `XMLHttpRequest` check before sending a request (see `csp`). No-op
    /// on a plain [`Context::new`]/`with_dom` that never calls it — nothing
    /// enforces a CSP until a host provides one, same pattern
    /// [`Context::set_url`] already has.
    pub fn set_csp(&mut self, policy: &str) {
        if let Some(state) = self._host_state.as_mut() {
            state.csp = Some(policy.to_string());
        }
    }

    /// Every rule text from every real `CSSStyleSheet` instance currently
    /// in `document.adoptedStyleSheets`, concatenated with newlines — a
    /// host (`profile-worker`'s `Page::layout`) parses this and merges it
    /// into the real cascade before laying out, so a page's own
    /// `sheet.insertRule(...)`/`deleteRule(...)` calls actually change what
    /// gets rendered on the next frame. A non-`CSSStyleSheet` entry in the
    /// array (nothing validates what a page assigns there) is silently
    /// skipped. Empty string on a plain [`Context::new`]/`with_dom` with
    /// nothing adopted.
    pub fn adopted_stylesheet_text(&self) -> String {
        unsafe {
            let global = sys::JS_GetGlobalObject(self.ptr);
            let doc_name = CString::new("document").unwrap();
            let document = sys::JS_GetPropertyStr(self.ptr, global, doc_name.as_ptr());
            sys::JS_FreeValue(self.ptr, global);
            let sheets_name = CString::new("adoptedStyleSheets").unwrap();
            let sheets = sys::JS_GetPropertyStr(self.ptr, document, sheets_name.as_ptr());
            sys::JS_FreeValue(self.ptr, document);

            let mut len: i64 = 0;
            sys::JS_GetLength(self.ptr, sheets, &mut len);
            let mut text = String::new();
            for i in 0..len.max(0) as u32 {
                let sheet = sys::JS_GetPropertyUint32(self.ptr, sheets, i);
                if let Some(rules) = cssom_stylesheet::rules_of(self.ptr, sheet) {
                    for rule in rules {
                        text.push_str(&rule);
                        text.push('\n');
                    }
                }
                sys::JS_FreeValue(self.ptr, sheet);
            }
            sys::JS_FreeValue(self.ptr, sheets);
            text
        }
    }

    /// Fires a real, cancelable `beforeunload` on `window` (the global
    /// object) — a host that's about to replace this context's page
    /// (`profile-worker`'s `RELOAD`/`NAVIGATE` handling) calls this first
    /// and only proceeds if it returns `true`. A listener that calls
    /// `event.preventDefault()` makes this return `false`, i.e. the page
    /// asked to stay. Real deviation from spec: a real browser then shows a
    /// confirmation dialog the user can override; this engine has no
    /// dialog/UI-confirmation surface at all yet (see
    /// `JS_ENGINE_CAPABILITY_MATRIX.md`'s "dialogs, popups" gap), so
    /// `preventDefault()` cancels the navigation outright rather than just
    /// requesting confirmation.
    pub fn fire_before_unload(&self) -> bool {
        unsafe {
            let global = sys::JS_GetGlobalObject(self.ptr);
            let proceed = events::dispatch_simple(self.ptr, global, "beforeunload", false, true);
            sys::JS_FreeValue(self.ptr, global);
            proceed
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
            timers::cleanup(self.ptr);
            fetch_async::cleanup(self.ptr);
            blob::cleanup(self.ptr);
            notifications::cleanup(self.ptr);
            dom_bindings::cleanup(self.ptr);
            sys::JS_FreeContext(self.ptr);
        }
    }
}
