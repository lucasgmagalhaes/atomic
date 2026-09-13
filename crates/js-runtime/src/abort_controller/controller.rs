//! `AbortController` class — split out from `abort_controller.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::helpers::{get_prop, resolve_prototype, set_prop, AND_SIGNAL};
use super::signal::{fire_abort, make_signal, resolve_reason};

pub(super) unsafe extern "C" fn controller_constructor(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let proto = resolve_prototype(ctx, new_target, "AbortController");
    let obj = sys::JS_NewObject(ctx);
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);
    let signal_proto = resolve_prototype(ctx, sys::js_undefined(), "AbortSignal");
    let signal = make_signal(ctx, signal_proto);
    sys::JS_FreeValue(ctx, signal_proto);
    set_prop(ctx, obj, AND_SIGNAL, signal);
    obj
}

pub(super) unsafe extern "C" fn controller_signal_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, AND_SIGNAL)
}

pub(super) unsafe extern "C" fn controller_abort(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let signal = get_prop(ctx, this_val, AND_SIGNAL);
    if signal.tag == sys::JS_TAG_OBJECT {
        let reason = resolve_reason(ctx, argc, argv);
        fire_abort(ctx, signal, reason);
    }
    sys::JS_FreeValue(ctx, signal);
    sys::js_undefined()
}
