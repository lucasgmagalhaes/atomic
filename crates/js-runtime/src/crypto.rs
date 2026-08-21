//! `crypto.getRandomValues(typedArray)`, scoped to `Uint8Array` (the
//! spec allows any integer TypedArray; the other element sizes are a
//! straightforward follow-up once one is actually needed).
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

unsafe extern "C" fn get_random_values(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let array = *argv;

    let mut size: usize = 0;
    let ptr = sys::JS_GetUint8Array(ctx, &mut size, array);
    if ptr.is_null() {
        // Not a Uint8Array. JS_GetUint8Array may have already raised a
        // TypeError on ctx; we don't inspect/propagate it (no
        // JS_GetException binding yet, see js-runtime's Context::eval).
        return sys::js_undefined();
    }

    let buf = std::slice::from_raw_parts_mut(ptr, size);
    if platform_apis::fill_random(buf).is_err() {
        return sys::js_undefined();
    }

    // getRandomValues returns the same array it filled in place, so callers
    // can write `let a = crypto.getRandomValues(new Uint8Array(4))`. argv[0]
    // is borrowed (JSValueConst) - JS_DupValue gives us our own reference to
    // return, since returning a JSValue transfers ownership to the caller.
    sys::JS_DupValue(ctx, array)
}

/// Registers `crypto.getRandomValues` as a global on `ctx`.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    let crypto = sys::JS_NewObject(ctx);

    let fn_name = CString::new("getRandomValues").unwrap();
    let func = sys::JS_NewCFunction2(
        ctx,
        get_random_values,
        fn_name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, crypto, fn_name.as_ptr(), func);

    let crypto_name = CString::new("crypto").unwrap();
    sys::JS_SetPropertyStr(ctx, global, crypto_name.as_ptr(), crypto);

    sys::JS_FreeValue(ctx, global);
}
