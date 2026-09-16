//! `pane("name")` → a `Pane` object with `goto(url)`, `click(selector)`,
//! and `fill(selector, value)`, all real (see this crate's top-level doc
//! for the two scope cuts that carry through from `profile-worker`'s own
//! protocol: `#id`-only selectors, `fill` setting `textContent` not a real
//! `.value`). Mirrors `js-runtime`'s
//! `dom_bindings::Node` class pattern: a custom quickjs class whose opaque
//! data is a boxed pane name, resolved back to a `profile::Profile` via
//! this module's own thread-local `PANES` registry (keyed by `JSContext`
//! pointer, set once in `register`, pointing at `AutomationEngine`'s boxed
//! pane map — see that struct's doc comment on why the box makes this
//! pointer stable). Deliberately *not* `JS_SetContextOpaque` — see the
//! `PANES` doc comment below for the real corruption bug that caused.
use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use neutron::quickjs_sys as sys;

type Panes = std::collections::HashMap<String, std::rc::Rc<std::cell::RefCell<profile::Profile>>>;

// Keyed by `JSContext` pointer, same thread_local convention `events`/
// `cron` already use in this crate — NOT `JS_SetContextOpaque`, which an
// earlier version of this module used. That was a real, confirmed bug:
// `js_runtime`'s own modules (e.g. `console::push_message`, invoked on
// every uncaught exception via `Context::eval`'s error-reporting path)
// unconditionally read the context opaque slot as `*mut HostState` and
// dereference it whenever it's non-null — a bare `Context::new` (what
// `AutomationEngine` always builds) leaves that slot null specifically so
// those reads degrade to a no-op. Stashing this crate's own `Panes`
// pointer there instead made every such read reinterpret an unrelated
// `HashMap`'s memory as a `HostState` and write through it — real,
// reproducible heap corruption (`STATUS_ACCESS_VIOLATION`/
// `STATUS_STACK_BUFFER_OVERRUN` depending on layout), triggered by
// nothing more exotic than any script throwing an uncaught exception.
thread_local! {
    static PANES: RefCell<std::collections::HashMap<usize, *mut Panes>> =
        RefCell::new(std::collections::HashMap::new());
}

// Per-`JSRuntime` class ID, not a `static ..._CLASS_ID: AtomicU32` shared
// across every `Runtime` in the process — that design was unsound under
// concurrent `Runtime` construction (see `js_runtime`'s `class_registry`
// module doc, which fixed the exact same bug for `Blob`/`Node`/etc: two
// runtimes racing `JS_NewClassID` is genuine UB, and even with a lock
// around just that call, one runtime's own class-id counter can end up
// out of step with a different class kind it allocates concurrently).
// `neutron::js::ensure_external_class`/`external_class_id` key by
// `(runtime pointer, kind)` instead so this crate gets the same safety
// without duplicating the bug.
const PANE_CLASS_KIND: &str = "automation::Pane";

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
    let mut len: usize = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, val, false);
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
    let s = String::from_utf8_lossy(bytes).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

// `quickjs-sys` only binds the low-level `JS_Throw(ctx, JSValue)`, not the
// `JS_ThrowTypeError`/`JS_NewError` convenience wrappers quickjs-ng also
// has — those aren't bound yet (nothing in this workspace has needed them
// before). Throwing a plain string is a real exception a script's `catch`
// still observes, just not an `instanceof Error` — same "no real error
// value yet" gap `Context::eval`'s own doc comment already flags.
unsafe fn throw(ctx: *mut sys::JSContext, message: &str) -> sys::JSValue {
    let msg = sys::JS_NewStringLen(
        ctx,
        message.as_ptr() as *const std::os::raw::c_char,
        message.len(),
    );
    sys::JS_Throw(ctx, msg)
}

unsafe fn pane_name<'a>(class_id: sys::JSClassID, this_val: sys::JSValue) -> Option<&'a str> {
    let ptr = sys::JS_GetOpaque(this_val, class_id) as *const String;
    ptr.as_ref().map(String::as_str)
}

unsafe fn with_pane<R>(
    ctx: *mut sys::JSContext,
    class_id: sys::JSClassID,
    this_val: sys::JSValue,
    f: impl FnOnce(&mut profile::Profile) -> R,
) -> Result<R, String> {
    let name = pane_name(class_id, this_val).ok_or("pane object missing its name")?;
    let panes = PANES.with(|reg| reg.borrow().get(&(ctx as usize)).copied());
    let Some(panes) = panes else {
        return Err("automation engine not initialized".to_string());
    };
    match (*panes).get(name) {
        Some(cell) => Ok(f(&mut cell.borrow_mut())),
        None => Err(format!("no pane named \"{name}\"")),
    }
}

unsafe extern "C" fn pane_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let class_id = neutron::js::external_class_id(rt, PANE_CLASS_KIND);
    let ptr = sys::JS_GetOpaque(val, class_id) as *mut String;
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

unsafe extern "C" fn pane_goto(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw(ctx, "pane.goto(url) requires a URL");
    }
    let Some(url) = read_js_string(ctx, *argv) else {
        return throw(ctx, "pane.goto(url): url must be a string");
    };
    let class_id = neutron::js::external_class_id(sys::JS_GetRuntime(ctx), PANE_CLASS_KIND);
    match with_pane(ctx, class_id, this_val, |profile| profile.navigate(&url)) {
        Ok(Ok(Ok(()))) => sys::js_undefined(),
        Ok(Ok(Err(message))) => throw(ctx, &format!("pane.goto failed: {message}")),
        Ok(Err(io_err)) => throw(ctx, &format!("pane.goto: worker unreachable: {io_err}")),
        Err(message) => throw(ctx, &message),
    }
}

unsafe extern "C" fn pane_fill(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw(
            ctx,
            "pane.fill(selector, value) requires a selector and a value",
        );
    }
    let Some(selector) = read_js_string(ctx, *argv) else {
        return throw(ctx, "pane.fill(selector, value): selector must be a string");
    };
    let Some(value) = read_js_string(ctx, *argv.add(1)) else {
        return throw(ctx, "pane.fill(selector, value): value must be a string");
    };
    let class_id = neutron::js::external_class_id(sys::JS_GetRuntime(ctx), PANE_CLASS_KIND);
    // Only `#id` selectors and a `textContent` assignment, not a real
    // `HTMLInputElement.value` — see `profile-worker`'s own doc on the
    // `FILL` command for why.
    match with_pane(ctx, class_id, this_val, |profile| {
        profile.fill(&selector, &value)
    }) {
        Ok(Ok(Ok(()))) => sys::js_undefined(),
        Ok(Ok(Err(message))) => throw(ctx, &format!("pane.fill failed: {message}")),
        Ok(Err(io_err)) => throw(ctx, &format!("pane.fill: worker unreachable: {io_err}")),
        Err(message) => throw(ctx, &message),
    }
}

unsafe extern "C" fn pane_click(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw(ctx, "pane.click(selector) requires a selector");
    }
    let Some(selector) = read_js_string(ctx, *argv) else {
        return throw(ctx, "pane.click(selector): selector must be a string");
    };
    let class_id = neutron::js::external_class_id(sys::JS_GetRuntime(ctx), PANE_CLASS_KIND);
    match with_pane(ctx, class_id, this_val, |profile| profile.click(&selector)) {
        Ok(Ok(Ok(()))) => sys::js_undefined(),
        Ok(Ok(Err(message))) => throw(ctx, &format!("pane.click failed: {message}")),
        Ok(Err(io_err)) => throw(ctx, &format!("pane.click: worker unreachable: {io_err}")),
        Err(message) => throw(ctx, &message),
    }
}

/// Registers the `Pane` class on `ctx`'s runtime (if not already done for
/// this runtime) and builds this context's `Pane.prototype`. Mirrors
/// `dom_bindings::ensure_node_class` exactly.
unsafe fn ensure_pane_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);

    let class_name = CString::new("Pane").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(pane_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = neutron::js::ensure_external_class(rt, PANE_CLASS_KIND, &def);

    let proto = sys::JS_NewObject(ctx);
    add_method(ctx, proto, "goto", pane_goto, 1);
    add_method(ctx, proto, "fill", pane_fill, 2);
    add_method(ctx, proto, "click", pane_click, 1);
    sys::JS_SetClassProto(ctx, class_id, proto);

    class_id
}

unsafe fn add_method(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
    name: &str,
    func: sys::JSCFunction,
    length: c_int,
) {
    let name_c = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(ctx, func, name_c.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, proto, name_c.as_ptr(), f);
}

unsafe extern "C" fn pane_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw(ctx, "pane(name) requires a pane name");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw(ctx, "pane(name): name must be a string");
    };

    let class_id = neutron::js::external_class_id(sys::JS_GetRuntime(ctx), PANE_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    let boxed = Box::new(name);
    sys::JS_SetOpaque(obj, Box::into_raw(boxed) as *mut c_void);
    obj
}

/// Registers the `pane` global and its `Pane` class on `ctx`, and records
/// `panes` in this module's own thread-local registry (keyed by `ctx`'s
/// address) — deliberately not `JS_SetContextOpaque`, see this module's
/// top doc for why that was a real bug.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext, panes: *mut Panes) {
    PANES.with(|reg| reg.borrow_mut().insert(ctx as usize, panes));
    ensure_pane_class(ctx);

    let global = sys::JS_GetGlobalObject(ctx);
    let name_c = CString::new("pane").unwrap();
    let f = sys::JS_NewCFunction2(
        ctx,
        pane_constructor,
        name_c.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, global, name_c.as_ptr(), f);
    sys::JS_FreeValue(ctx, global);
}

/// Removes `ctx`'s entry from the pane registry. Must be called before
/// `ctx` is freed (from `AutomationEngine::drop`, alongside `events::
/// cleanup`/`cron::cleanup`) — Windows readily reuses a freed
/// `JSContext`'s address for the next one, and a stale entry at that
/// address would let a brand new, unrelated context's `pane(...)` calls
/// resolve against this engine's already-dropped panes.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    PANES.with(|reg| {
        reg.borrow_mut().remove(&(ctx as usize));
    });
}
