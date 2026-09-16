//! `read_js_number`/`read_js_string`/`register_fn` — split out from
//! `chrome_bridge.rs`.

use std::ffi::CString;
use std::os::raw::c_int;

use neutron::quickjs_sys as sys;

/// Same "stringify then parse" trick this codebase already uses elsewhere
/// (see `profile_worker::input_commands::read_js_string`-shaped helpers) —
/// no safe binding for `JS_ToFloat64` exists in `quickjs-sys` yet, and this
/// is a plain numeric argument, not a hot path.
pub(super) unsafe fn read_js_number(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<f64> {
    read_js_string(ctx, val).and_then(|s| s.trim().parse::<f64>().ok())
}

/// Same "stringify via `JS_ToCStringLen2`" convention `automation::cron`'s
/// own `read_js_string` uses for reading an arbitrary JS argument back into
/// Rust.
pub(super) unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
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

/// Registers one `atomic.<name>` native function on `atomic` — the shared
/// primitive [`super::register`] builds every entry from, added when the
/// list grew past the point where inlining each `CString`/
/// `JS_NewCFunction2`/`JS_SetPropertyStr` triplet by hand was worth it.
pub(super) unsafe fn register_fn(
    ctx: *mut sys::JSContext,
    atomic: sys::JSValue,
    name: &str,
    func: sys::JSCFunction,
    arity: c_int,
) {
    let name_c = CString::new(name).expect("bridge function name must not contain NUL bytes");
    let f = sys::JS_NewCFunction2(ctx, func, name_c.as_ptr(), arity, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, atomic, name_c.as_ptr(), f);
}
