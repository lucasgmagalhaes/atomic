//! `navigator.clipboard.writeText`/`readText` — real OS clipboard access,
//! backed by `platform_apis::clipboard_write_text`/`clipboard_read_text`
//! (which already wraps `arboard`; this module is just the JS binding
//! that crate's own doc comment flagged as "not wired up yet"). Text
//! only, matching the real `Clipboard` API's own `writeText`/`readText`
//! (no `write`/`read` with images/HTML, no `ClipboardItem`).
//!
//! Both methods return real Promises (`JS_NewPromiseCapability`),
//! resolved/rejected synchronously in the same call — a clipboard round
//! trip is a fast local OS call, not network I/O, so unlike `fetch()`
//! there's no reason to hand it to a background thread; it just needs to
//! be Promise-shaped because that's the real API's contract (a page can
//! legitimately `await navigator.clipboard.writeText(...)`).
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

unsafe fn define_method(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &str, func: sys::JSCFunction, length: c_int) {
    let name_c = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(ctx, func, name_c.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, obj, name_c.as_ptr(), f);
}

/// Settles a fresh Promise immediately: `Ok(value)` resolves with
/// `value` (transfers ownership), `Err(message)` rejects with a plain
/// `Error(message)` object.
unsafe fn settled_promise(ctx: *mut sys::JSContext, result: Result<sys::JSValue, String>) -> sys::JSValue {
    let mut resolving_funcs = [sys::js_undefined(); 2];
    let promise = sys::JS_NewPromiseCapability(ctx, resolving_funcs.as_mut_ptr());
    let [resolve, reject] = resolving_funcs;

    let (settle_fn, mut arg) = match result {
        Ok(value) => (resolve, value),
        Err(message) => {
            let name = CString::new("Error").unwrap();
            let global = sys::JS_GetGlobalObject(ctx);
            let error_ctor = sys::JS_GetPropertyStr(ctx, global, name.as_ptr());
            sys::JS_FreeValue(ctx, global);
            let mut msg_arg = new_js_string(ctx, &message);
            let error_obj = sys::JS_Call(ctx, error_ctor, sys::js_undefined(), 1, &mut msg_arg);
            sys::JS_FreeValue(ctx, msg_arg);
            sys::JS_FreeValue(ctx, error_ctor);
            (reject, error_obj)
        }
    };

    let settle_result = sys::JS_Call(ctx, settle_fn, sys::js_undefined(), 1, &mut arg);
    sys::JS_FreeValue(ctx, settle_result);
    sys::JS_FreeValue(ctx, arg);
    sys::JS_FreeValue(ctx, resolve);
    sys::JS_FreeValue(ctx, reject);
    promise
}

unsafe extern "C" fn write_text(ctx: *mut sys::JSContext, _this_val: sys::JSValue, argc: c_int, argv: *mut sys::JSValue) -> sys::JSValue {
    if !crate::permissions_policy::is_allowed(ctx, "clipboard-write") {
        return settled_promise(ctx, Err("clipboard write is blocked by Permissions Policy".to_string()));
    }
    let text = if argc >= 1 { read_js_string(ctx, *argv) } else { None };
    let Some(text) = text else {
        return settled_promise(ctx, Err("writeText: missing text argument".to_string()));
    };

    match platform_apis::clipboard_write_text(&text) {
        Ok(()) => settled_promise(ctx, Ok(sys::js_undefined())),
        Err(e) => settled_promise(ctx, Err(format!("writeText failed: {e}"))),
    }
}

unsafe extern "C" fn read_text(ctx: *mut sys::JSContext, _this_val: sys::JSValue, _argc: c_int, _argv: *mut sys::JSValue) -> sys::JSValue {
    if !crate::permissions_policy::is_allowed(ctx, "clipboard-read") {
        return settled_promise(ctx, Err("clipboard read is blocked by Permissions Policy".to_string()));
    }
    match platform_apis::clipboard_read_text() {
        Ok(text) => settled_promise(ctx, Ok(new_js_string(ctx, &text))),
        Err(e) => settled_promise(ctx, Err(format!("readText failed: {e}"))),
    }
}

/// Registers `navigator.clipboard.writeText`/`readText` as globals.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);

    let navigator_name = CString::new("navigator").unwrap();
    let navigator = sys::JS_GetPropertyStr(ctx, global, navigator_name.as_ptr());
    let navigator = if navigator.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, navigator);
        let obj = sys::JS_NewObject(ctx);
        sys::JS_SetPropertyStr(ctx, global, navigator_name.as_ptr(), sys::JS_DupValue(ctx, obj));
        obj
    } else {
        navigator
    };

    let clipboard = sys::JS_NewObject(ctx);
    define_method(ctx, clipboard, "writeText", write_text, 1);
    define_method(ctx, clipboard, "readText", read_text, 0);

    let clipboard_name = CString::new("clipboard").unwrap();
    sys::JS_SetPropertyStr(ctx, navigator, clipboard_name.as_ptr(), clipboard);

    sys::JS_FreeValue(ctx, navigator);
    sys::JS_FreeValue(ctx, global);
}
