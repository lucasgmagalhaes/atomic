//! Minimal DOM↔JS bindings: two global functions backed by a `dom::Dom`
//! stashed in the context's opaque slot. Deliberately not the real
//! `document.getElementById`/`Node.textContent` API — those need JS-side
//! object identity for DOM nodes (a node "class" with a `NodeId` handle),
//! which doesn't exist yet. This proves the FFI wiring (native function
//! registration, argument/return marshaling, reaching Rust-owned state
//! from a callback) ahead of designing that object model.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

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

unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe extern "C" fn dom_get_text_by_id(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_null();
    }
    let Some(id) = read_js_string(ctx, *argv) else {
        return sys::js_null();
    };

    let dom_ptr = sys::JS_GetContextOpaque(ctx) as *mut dom::Dom;
    if dom_ptr.is_null() {
        return sys::js_null();
    }
    let dom_ref = &*dom_ptr;

    match dom_ref.find_by_id(&id) {
        Some(node) => new_js_string(ctx, &dom_ref.text_content(node)),
        None => sys::js_null(),
    }
}

unsafe extern "C" fn dom_set_text_by_id(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return sys::js_bool(false);
    }
    let Some(id) = read_js_string(ctx, *argv) else {
        return sys::js_bool(false);
    };
    let Some(text) = read_js_string(ctx, *argv.add(1)) else {
        return sys::js_bool(false);
    };

    let dom_ptr = sys::JS_GetContextOpaque(ctx) as *mut dom::Dom;
    if dom_ptr.is_null() {
        return sys::js_bool(false);
    }

    let found = (*dom_ptr).find_by_id(&id);
    match found {
        Some(node) => {
            (*dom_ptr).set_text_content(node, &text);
            sys::js_bool(true)
        }
        None => sys::js_bool(false),
    }
}

/// Registers `__dom_get_text_by_id(id)` and `__dom_set_text_by_id(id, text)`
/// as globals on `ctx`. Callers must have already pointed the context's
/// opaque slot at a live `dom::Dom` via `JS_SetContextOpaque` — these
/// functions read it back on every call and no-op (return null/false) if
/// it's unset.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);

    let get_name = CString::new("__dom_get_text_by_id").unwrap();
    let get_fn = sys::JS_NewCFunction2(
        ctx,
        dom_get_text_by_id,
        get_name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, global, get_name.as_ptr(), get_fn);

    let set_name = CString::new("__dom_set_text_by_id").unwrap();
    let set_fn = sys::JS_NewCFunction2(
        ctx,
        dom_set_text_by_id,
        set_name.as_ptr(),
        2,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, global, set_name.as_ptr(), set_fn);

    sys::JS_FreeValue(ctx, global);
}
