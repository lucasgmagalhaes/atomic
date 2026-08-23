//! Safe wrapper around `quickjs-sys`. Minimal on purpose (phase 2 slice):
//! create a runtime/context and eval JS to a string. DOM↔JS bindings land
//! in a later pass once `dom` has something worth exposing.

use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::path::Path;

use quickjs_sys as sys;

mod crypto;
mod document;
mod document_cookie;
mod dom_bindings;
mod events;
mod fetch;
mod fetch_async;
mod host_state;
mod indexed_db_bindings;
mod local_storage_bindings;
mod page_visibility;
mod performance;
mod timers;
mod value_bridge;

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
            page_visibility::register(ptr);
            fetch::register(ptr);
            fetch_async::register(ptr);
            timers::register(ptr);
            document_cookie::register(ptr);
            indexed_db_bindings::register(ptr);
            local_storage_bindings::register(ptr);
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
        });
        let raw = state.as_mut() as *mut host_state::HostState as *mut std::os::raw::c_void;
        unsafe {
            sys::JS_SetContextOpaque(ctx.ptr, raw);
            dom_bindings::register(ctx.ptr);
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
}

impl Drop for Context<'_> {
    fn drop(&mut self) {
        unsafe {
            timers::cleanup(self.ptr);
            fetch_async::cleanup(self.ptr);
            sys::JS_FreeContext(self.ptr);
        }
    }
}
