//! `Runtime` — the `quickjs-sys` runtime wrapper `Context` is built on top
//! of, plus the external-class-registration helpers other crates that
//! embed a `Runtime`/`Context` use. Split out from `lib.rs`.

use quickjs_sys as sys;

use crate::{class_registry, module_loader, shared_worker_bindings};

#[derive(Debug)]
pub struct EvalError(pub String);

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JS exception: {}", self.0)
    }
}

impl std::error::Error for EvalError {}

pub struct Runtime {
    pub(crate) ptr: *mut sys::JSRuntime,
}

impl Runtime {
    pub fn new() -> Self {
        let ptr = unsafe { sys::JS_NewRuntime() };
        assert!(!ptr.is_null(), "JS_NewRuntime returned null");
        // Real ES module linking (`ROADMAP.md` item 19): registering this
        // one normalize/load callback pair is what makes both static
        // `import` and dynamic `import()` work — quickjs-ng has no
        // separate dynamic-import host hook in this version (confirmed by
        // grep against the vendored header), so both syntaxes route
        // through here.
        unsafe {
            sys::JS_SetModuleLoaderFunc(
                ptr,
                Some(module_loader::module_normalize_fn),
                Some(module_loader::module_load_fn),
                std::ptr::null_mut(),
            );
        }
        Runtime { ptr }
    }
}

/// Registers (or looks up, if already registered for this specific
/// `JSRuntime`) a QuickJS class under `kind`, going through the same
/// per-runtime registry every class in this crate uses — see
/// `class_registry`'s module doc for why a `static ..._CLASS_ID` per
/// class kind is unsound under concurrent `Runtime` construction. `kind`
/// must be a value unique to the caller (e.g. `"automation::Pane"`) so it
/// can't collide with a class kind registered from inside this crate.
/// Exposed so other crates that embed a `js_runtime::Runtime`/`Context`
/// (e.g. `automation`'s `Pane` class) get the same safety this crate's
/// own bindings have, instead of reinventing the same bug.
pub unsafe fn ensure_external_class(
    rt: *mut sys::JSRuntime,
    kind: &'static str,
    def: &sys::JSClassDef,
) -> sys::JSClassID {
    class_registry::ensure_class(rt, kind, def)
}

/// Looks up a class ID previously registered via
/// [`ensure_external_class`] for `rt`/`kind`. Returns `0`
/// (`JS_INVALID_CLASS_ID`) if never registered for this runtime.
pub fn external_class_id(rt: *mut sys::JSRuntime, kind: &'static str) -> sys::JSClassID {
    class_registry::class_id_for(rt, kind)
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // JS_FreeRuntime must run first: it triggers finalizers for any
        // surviving GC objects, and those finalizers look up opaque
        // pointers via class_id_for in this runtime's registry.
        // cleanup_runtime after — the registry is dead (Runtime is
        // !Send/!Sync, so no cross-thread reuse is possible).
        unsafe { sys::JS_FreeRuntime(self.ptr) };
        class_registry::cleanup_runtime(self.ptr);
        // Same pointer-reuse hazard class_registry's own comment above
        // documents: a SharedWorker instance keyed by this runtime's raw
        // pointer must be evicted (and its OS thread joined) before that
        // address can be handed to a later, unrelated `JS_NewRuntime()`
        // call — see `shared_worker_bindings::evict_runtime`'s own doc.
        shared_worker_bindings::evict_runtime(self.ptr as usize);
    }
}

// The runtime owns no thread-local state we rely on here, but QuickJS
// runtimes are not meant to be touched from multiple threads at once.
// Leave Runtime !Send/!Sync (the default for a raw-pointer field) until
// the profile/ipc process-per-tab model defines how this actually gets
// used across threads.
