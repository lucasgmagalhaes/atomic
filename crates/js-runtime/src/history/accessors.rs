//! `length`/`state` getters and `define_readonly` — split out from
//! `history.rs`.

use std::ffi::CString;

use quickjs_sys as sys;

use crate::js_helpers::Getter;

use super::helpers::{entries, entry_at, entry_field, index};

pub(super) unsafe extern "C" fn length_get(
    ctx: *mut sys::JSContext,
    this: sys::JSValue,
) -> sys::JSValue {
    let list = entries(ctx, this);
    let mut len = 0;
    sys::JS_GetLength(ctx, list, &mut len);
    sys::JS_FreeValue(ctx, list);
    sys::js_float64(len as f64)
}
pub(super) unsafe extern "C" fn state_get(
    ctx: *mut sys::JSContext,
    this: sys::JSValue,
) -> sys::JSValue {
    let current = index(ctx, this);
    let entry = entry_at(ctx, this, current);
    let state = entry_field(ctx, entry, "state");
    sys::JS_FreeValue(ctx, entry);
    state
}

pub(super) unsafe fn define_readonly(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    getter: Getter,
) {
    let cname = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(getter),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        obj,
        atom,
        f,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}
