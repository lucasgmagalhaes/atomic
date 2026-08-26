//! Shared string/error helpers and the `Event` native-class getter-binding
//! helper, used by two or more `events` submodules.

use quickjs_sys as sys;
use std::ffi::CString;

pub(super) type Getter = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue) -> sys::JSValue;

pub(super) unsafe fn string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}

pub(super) unsafe fn type_error(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, string(ctx, s))
}

pub(super) unsafe fn read_string(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<String> {
    let mut len = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, value, false);
    if ptr.is_null() {
        return None;
    };
    let s = String::from_utf8_lossy(std::slice::from_raw_parts(ptr as *const u8, len)).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

pub(super) unsafe fn getter(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
    name: &str,
    func: Getter,
) {
    let name = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(func),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        f,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom)
}
