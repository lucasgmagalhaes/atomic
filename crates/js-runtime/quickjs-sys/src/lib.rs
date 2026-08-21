//! Minimal raw FFI bindings to QuickJS-ng, hand-written against the subset
//! of `quickjs.h` this workspace needs (runtime/context lifecycle + eval).
//! Not a full binding — extend as `js-runtime` needs more of the C API.
//!
//! Only covers 64-bit targets (no `JS_NAN_BOXING`, which quickjs.h only
//! enables when `INTPTR_MAX < INT64_MAX`). All three target platforms
//! (Windows/Linux/macOS desktop) build 64-bit, so this is not a limitation
//! in practice — it just means `JSValue`'s layout below would be wrong on
//! a 32-bit target and must not be assumed portable to one.
#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int, c_void};

#[repr(C)]
#[derive(Clone, Copy)]
pub union JSValueUnion {
    pub int32: i32,
    pub float64: f64,
    pub ptr: *mut c_void,
    pub short_big_int: i32,
}

/// Mirrors quickjs.h's `JSValue` bit-for-bit (64-bit, non-NAN-boxed layout).
/// This is the C struct's raw representation, not an owned Rust value: it
/// carries no refcounting of its own, so `Copy` is safe at the type level —
/// but copying it does NOT duplicate the underlying JS reference. Callers
/// must still track ownership per quickjs.h's `JSValue`/`JSValueConst`
/// convention (see quickjs.h's own comment on `JS_CHECK_JSVALUE`) and call
/// `JS_FreeValue` exactly once per value the C API transferred ownership of.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct JSValue {
    pub u: JSValueUnion,
    pub tag: i64,
}

pub const JS_TAG_EXCEPTION: i64 = 6;

pub const JS_EVAL_TYPE_GLOBAL: c_int = 0;

#[repr(C)]
pub struct JSRuntime {
    _private: [u8; 0],
}

#[repr(C)]
pub struct JSContext {
    _private: [u8; 0],
}

extern "C" {
    pub fn JS_NewRuntime() -> *mut JSRuntime;
    pub fn JS_FreeRuntime(rt: *mut JSRuntime);

    pub fn JS_NewContext(rt: *mut JSRuntime) -> *mut JSContext;
    pub fn JS_FreeContext(ctx: *mut JSContext);

    pub fn JS_Eval(
        ctx: *mut JSContext,
        input: *const c_char,
        input_len: usize,
        filename: *const c_char,
        eval_flags: c_int,
    ) -> JSValue;

    pub fn JS_FreeValue(ctx: *mut JSContext, v: JSValue);

    pub fn JS_ToCStringLen2(
        ctx: *mut JSContext,
        plen: *mut usize,
        val: JSValue,
        cesu8: bool,
    ) -> *const c_char;
    pub fn JS_FreeCString(ctx: *mut JSContext, ptr: *const c_char);
}

/// Safe-ish helper mirroring the `JS_IsException` static inline from
/// quickjs.h (tag comparison only, no ownership implications).
pub fn js_is_exception(v: &JSValue) -> bool {
    v.tag == JS_TAG_EXCEPTION
}
