//! ES Module resource loader + cache (`ROADMAP.md` item 19;
//! `.claude/plans/architecture-p5-foundations.plan.md` Stage 4).
//!
//! Scope cut, deliberate: this lands the resolver + fetch-and-cache
//! primitive only — real module semantics (import/export binding,
//! linking, a module namespace object, `import()` returning a real
//! `Promise` resolving to that namespace, circular-dependency handling)
//! stay item 19's own remaining work, built on top of this rather than
//! this stage inventing a parallel mechanism
//! (`architecture/overview.md`'s "architecture before APIs" rule). What's
//! real here: a specifier resolves against the importing module's own
//! URL via `url::Url::join` — the exact primitive `URL`'s own constructor
//! (`url_bindings::url_constructor`) already uses for its `base`
//! argument — and a module's source text is fetched at most once per
//! `Context` and cached by resolved URL, the same "compute once, key by
//! an identity, reuse on a hit" shape `LayoutCache`/`PaintCache` already
//! use for layout/paint.
//!
//! Background fetch mirrors `fetch_async::fetch_fn`'s own shape (a real
//! OS thread via `net::request`, delivered through [`pump`] on the same
//! "host pumps explicitly, no real event loop yet" cadence every other
//! async-ish primitive in this crate already has) rather than a new
//! fetch mechanism.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::sync::mpsc;
use std::thread;

use quickjs_sys as sys;

/// A module fetch's real, observable state. `Loading` until [`pump`]
/// delivers a result; `Ready`/`Failed` are terminal — a caller who wants
/// to reload the same URL as a genuinely fresh fetch isn't supported
/// today (real ES module semantics only ever fetch a given URL once per
/// realm, matching this).
#[derive(Debug, Clone)]
pub enum ModuleStatus {
    Loading,
    Ready(String),
    Failed(String),
}

#[derive(Default)]
struct ModuleCache {
    records: HashMap<String, ModuleStatus>,
    pending: Vec<(String, mpsc::Receiver<Result<net::Response, net::Error>>)>,
}

thread_local! {
    // Keyed by JSContext pointer, same reasoning as `fetch_async::STATE`.
    static CACHES: RefCell<HashMap<usize, ModuleCache>> = RefCell::new(HashMap::new());
}

/// Resolves an `import`/dynamic-`import()` specifier against the
/// importing module's own URL. Real relative/absolute resolution, not a
/// string-concatenation approximation — `None` for an unparseable
/// `base_url` or `specifier` (a bare specifier with no import map —
/// ROADMAP item 19's own remaining work — can't resolve without one,
/// same as a real browser without one configured).
pub(crate) fn resolve_specifier(base_url: &str, specifier: &str) -> Option<String> {
    let base = url::Url::parse(base_url).ok()?;
    base.join(specifier).ok().map(|u| u.to_string())
}

/// Starts a background fetch of `resolved_url`'s source text, or is a
/// real no-op if it's already cached or in flight — the actual point of
/// this being a *cache*, not just a fetch wrapper: a module imported by
/// two different modules in the same page must not trigger two network
/// round trips.
pub(crate) fn load(ctx: *mut sys::JSContext, resolved_url: &str) {
    CACHES.with(|c| {
        let mut map = c.borrow_mut();
        let cache = map.entry(ctx as usize).or_default();
        if cache.records.contains_key(resolved_url) {
            return;
        }
        cache
            .records
            .insert(resolved_url.to_string(), ModuleStatus::Loading);
        let (tx, rx) = mpsc::channel();
        let url = resolved_url.to_string();
        thread::spawn(move || {
            let _ = tx.send(net::request("GET", &url, &[], None));
        });
        cache.pending.push((resolved_url.to_string(), rx));
    });
}

/// Real cache read side — `None` if [`load`] was never called for this
/// URL on this `Context`.
pub(crate) fn status(ctx: *mut sys::JSContext, resolved_url: &str) -> Option<ModuleStatus> {
    CACHES.with(|c| {
        c.borrow()
            .get(&(ctx as usize))
            .and_then(|cache| cache.records.get(resolved_url).cloned())
    })
}

/// Moves every completed background fetch's result into the cache — same
/// "host pumps explicitly" cadence every other async-ish primitive in
/// this crate already has. Returns how many module fetches completed.
pub(crate) fn pump(ctx: *mut sys::JSContext) -> usize {
    let due = CACHES.with(|c| {
        let mut map = c.borrow_mut();
        let Some(cache) = map.get_mut(&(ctx as usize)) else {
            return Vec::new();
        };
        let mut due = Vec::new();
        let mut i = 0;
        while i < cache.pending.len() {
            match cache.pending[i].1.try_recv() {
                Ok(result) => due.push((cache.pending.remove(i).0, result)),
                Err(_) => i += 1,
            }
        }
        due
    });
    if due.is_empty() {
        return 0;
    }
    let delivered = due.len();
    CACHES.with(|c| {
        let mut map = c.borrow_mut();
        let cache = map.entry(ctx as usize).or_default();
        for (url, result) in due {
            let record = match result {
                Ok(response) => {
                    ModuleStatus::Ready(String::from_utf8_lossy(&response.body).into_owned())
                }
                Err(e) => ModuleStatus::Failed(e.to_string()),
            };
            cache.records.insert(url, record);
        }
    });
    delivered
}

/// Real `JSModuleNormalizeFunc` (`ROADMAP.md` item 19) — registered once
/// per `Runtime` via `JS_SetModuleLoaderFunc` (`runtime.rs`). Resolves
/// `module_name` against `module_base_name` via [`resolve_specifier`]'s
/// real `url::Url::join`, returning a string allocated with
/// `sys::js_strdup` (quickjs-ng frees this with its own allocator, so a
/// plain `CString::into_raw` would be unsound here). Throws a real
/// `ReferenceError` and returns null on an unresolvable specifier (a bare
/// specifier with no import map, or an unparseable base) - same "can't
/// resolve without one" outcome `resolve_specifier`'s own doc documents.
pub(crate) unsafe extern "C" fn module_normalize_fn(
    ctx: *mut sys::JSContext,
    module_base_name: *const c_char,
    module_name: *const c_char,
    _opaque: *mut c_void,
) -> *mut c_char {
    let base = CStr::from_ptr(module_base_name).to_string_lossy();
    let specifier = CStr::from_ptr(module_name).to_string_lossy();
    match resolve_specifier(&base, &specifier) {
        Some(resolved) => match CString::new(resolved) {
            Ok(c) => sys::js_strdup(ctx, c.as_ptr()),
            Err(_) => std::ptr::null_mut(),
        },
        None => {
            let fmt = CString::new("could not resolve module specifier '%s'").unwrap();
            let spec_c = CString::new(specifier.into_owned()).unwrap_or_default();
            sys::JS_ThrowReferenceError(ctx, fmt.as_ptr(), spec_c.as_ptr());
            std::ptr::null_mut()
        }
    }
}

/// Real `JSModuleLoaderFunc` (`ROADMAP.md` item 19) — registered
/// alongside [`module_normalize_fn`]. Fetches `module_name` (already
/// normalized/resolved to an absolute URL) synchronously: quickjs-ng's
/// module-loader contract has no async continuation (it must return a
/// real `JSModuleDef*` or `NULL` immediately), so this blocks the calling
/// thread on the network request - a real, documented scope cut versus
/// Stage 4's non-blocking background-thread fetch (still consulted/
/// populated here, so a module already fetched once - by a prior
/// `load`/`pump` cycle, or an earlier import of the same URL in this same
/// graph - is reused instead of re-fetched). Compiles the fetched source
/// via `JS_Eval(..., JS_EVAL_TYPE_MODULE | JS_EVAL_FLAG_COMPILE_ONLY)`,
/// matching quickjs-libc.c's own reference `js_module_load`.
pub(crate) unsafe extern "C" fn module_load_fn(
    ctx: *mut sys::JSContext,
    module_name: *const c_char,
    _opaque: *mut c_void,
) -> *mut sys::JSModuleDef {
    let resolved = CStr::from_ptr(module_name).to_string_lossy().into_owned();

    let source = match status(ctx, &resolved) {
        Some(ModuleStatus::Ready(text)) => text,
        _ => match net::request("GET", &resolved, &[], None) {
            Ok(response) => {
                let text = String::from_utf8_lossy(&response.body).into_owned();
                CACHES.with(|c| {
                    c.borrow_mut()
                        .entry(ctx as usize)
                        .or_default()
                        .records
                        .insert(resolved.clone(), ModuleStatus::Ready(text.clone()));
                });
                text
            }
            Err(e) => {
                let message = e.to_string();
                CACHES.with(|c| {
                    c.borrow_mut()
                        .entry(ctx as usize)
                        .or_default()
                        .records
                        .insert(resolved.clone(), ModuleStatus::Failed(message.clone()));
                });
                let fmt = CString::new("could not load module '%s': %s").unwrap();
                let name_c = CString::new(resolved).unwrap_or_default();
                let msg_c = CString::new(message).unwrap_or_default();
                sys::JS_ThrowReferenceError(ctx, fmt.as_ptr(), name_c.as_ptr(), msg_c.as_ptr());
                return std::ptr::null_mut();
            }
        },
    };

    let source_c = match CString::new(source) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let name_c = match CString::new(resolved) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let result = sys::JS_Eval(
        ctx,
        source_c.as_ptr(),
        source_c.as_bytes().len(),
        name_c.as_ptr(),
        sys::JS_EVAL_TYPE_MODULE | sys::JS_EVAL_FLAG_COMPILE_ONLY,
    );
    if sys::js_is_exception(&result) {
        return std::ptr::null_mut();
    }
    // The module is already referenced by quickjs-ng's own internal
    // module table once compiled - this returned `JSValue` wrapper must
    // still be freed (matching quickjs-libc.c's own `js_module_load`),
    // but doing so does not free the module itself.
    let module = result.u.ptr as *mut sys::JSModuleDef;
    sys::JS_FreeValue(ctx, result);
    module
}

/// Frees `ctx`'s module cache — must run before `JS_FreeContext(ctx)`,
/// same ordering requirement every other module's `cleanup(ctx)` already
/// documents (nothing here holds a `JSValue`, so this is just a `HashMap`
/// drop, not a `JS_FreeValue` pass — still done at the same point in
/// `Context::drop` for consistency).
pub(crate) fn cleanup(ctx: *mut sys::JSContext) {
    CACHES.with(|c| {
        c.borrow_mut().remove(&(ctx as usize));
    });
}
