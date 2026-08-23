//! `pane("name")` → a `Pane` object with `goto(url)` (real) and
//! `fill(selector, value)`/`click(selector)` (stubbed — throw, don't
//! no-op; see this crate's top-level doc). Mirrors `js-runtime`'s
//! `dom_bindings::Node` class pattern: a custom quickjs class whose opaque
//! data is a boxed pane name, resolved back to a `profile::Profile` via the
//! context's opaque slot (set once in `register`, pointing at
//! `AutomationEngine`'s boxed pane map — see that struct's doc comment on
//! why the box makes this pointer stable).
use std::ffi::CString;
use std::os::raw::{c_int, c_void};
use std::sync::atomic::AtomicU32;

use quickjs_sys as sys;

static PANE_CLASS_ID: AtomicU32 = AtomicU32::new(0);

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
    let msg = sys::JS_NewStringLen(ctx, message.as_ptr() as *const std::os::raw::c_char, message.len());
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
    let panes = sys::JS_GetContextOpaque(ctx) as *mut std::collections::HashMap<String, &mut profile::Profile>;
    if panes.is_null() {
        return Err("automation engine not initialized".to_string());
    }
    match (*panes).get_mut(name) {
        // `profile` is `&mut &mut Profile` (a mutable ref into the map's
        // borrowed value) — reborrow through it rather than moving the
        // inner `&mut Profile` out, which the map still owns.
        Some(profile) => Ok(f(&mut **profile)),
        None => Err(format!("no pane named \"{name}\"")),
    }
}

unsafe extern "C" fn pane_finalizer(_rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let class_id = PANE_CLASS_ID.load(std::sync::atomic::Ordering::Relaxed);
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
    let class_id = PANE_CLASS_ID.load(std::sync::atomic::Ordering::Relaxed);
    match with_pane(ctx, class_id, this_val, |profile| profile.navigate(&url)) {
        Ok(Ok(Ok(()))) => sys::js_undefined(),
        Ok(Ok(Err(message))) => throw(ctx, &format!("pane.goto failed: {message}")),
        Ok(Err(io_err)) => throw(ctx, &format!("pane.goto: worker unreachable: {io_err}")),
        Err(message) => throw(ctx, &message),
    }
}

unsafe extern "C" fn pane_fill(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    // `profile-worker`'s stdin protocol has no form-input command yet (only
    // PING/RELOAD/NAVIGATE/QUIT — see the spec's own gap on input sync
    // between panes, CLAUDE.md line 252). Throwing here instead of
    // pretending this worked, matching this repo's "no fake success"
    // convention.
    throw(ctx, "pane.fill: not implemented — profile-worker has no input-injection command yet")
}

unsafe extern "C" fn pane_click(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    throw(ctx, "pane.click: not implemented — profile-worker has no input-injection command yet")
}

/// Registers the `Pane` class on `ctx`'s runtime (if not already done for
/// this runtime) and builds this context's `Pane.prototype`. Mirrors
/// `dom_bindings::ensure_node_class` exactly.
unsafe fn ensure_pane_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = sys::JS_NewClassID(rt, PANE_CLASS_ID.as_ptr());

    let class_name = CString::new("Pane").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(pane_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    sys::JS_NewClass(rt, class_id, &def);

    let proto = sys::JS_NewObject(ctx);
    add_method(ctx, proto, "goto", pane_goto, 1);
    add_method(ctx, proto, "fill", pane_fill, 2);
    add_method(ctx, proto, "click", pane_click, 1);
    sys::JS_SetClassProto(ctx, class_id, proto);

    class_id
}

unsafe fn add_method(ctx: *mut sys::JSContext, proto: sys::JSValue, name: &str, func: sys::JSCFunction, length: c_int) {
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

    let class_id = PANE_CLASS_ID.load(std::sync::atomic::Ordering::Relaxed);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    let boxed = Box::new(name);
    sys::JS_SetOpaque(obj, Box::into_raw(boxed) as *mut c_void);
    obj
}

/// Registers the `pane` global and its `Pane` class on `ctx`, and stashes
/// `panes` in the context's opaque slot — safe to do here (rather than
/// clashing with `js-runtime`'s own use of that slot for `HostState`)
/// because `AutomationEngine` always constructs a plain `Context::new`,
/// never `with_dom`/`with_storage`.
pub(crate) unsafe fn register(
    ctx: *mut sys::JSContext,
    panes: *mut std::collections::HashMap<String, &mut profile::Profile>,
) {
    sys::JS_SetContextOpaque(ctx, panes as *mut c_void);
    ensure_pane_class(ctx);

    let global = sys::JS_GetGlobalObject(ctx);
    let name_c = CString::new("pane").unwrap();
    let f = sys::JS_NewCFunction2(ctx, pane_constructor, name_c.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, global, name_c.as_ptr(), f);
    sys::JS_FreeValue(ctx, global);
}
