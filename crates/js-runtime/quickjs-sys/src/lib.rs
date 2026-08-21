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

pub const JS_TAG_BOOL: i64 = 1;
pub const JS_TAG_NULL: i64 = 2;
pub const JS_TAG_UNDEFINED: i64 = 3;
pub const JS_TAG_EXCEPTION: i64 = 6;
pub const JS_TAG_FLOAT64: i64 = 8;

pub const JS_EVAL_TYPE_GLOBAL: c_int = 0;

/// `JSCFunctionEnum::JS_CFUNC_generic` — the calling convention for a plain
/// `(ctx, this_val, argc, argv) -> JSValue` native function, as opposed to
/// the magic/data/constructor variants quickjs.h also defines.
pub const JS_CFUNC_GENERIC: c_int = 0;

/// Matches `JSCFunction` from quickjs.h: the signature every function
/// registered via `JS_NewCFunction2` with `JS_CFUNC_GENERIC` must have.
pub type JSCFunction = unsafe extern "C" fn(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue;

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

    pub fn JS_NewStringLen(ctx: *mut JSContext, str1: *const c_char, len1: usize) -> JSValue;

    pub fn JS_NewCFunction2(
        ctx: *mut JSContext,
        func: JSCFunction,
        name: *const c_char,
        length: c_int,
        cproto: c_int,
        magic: c_int,
    ) -> JSValue;

    pub fn JS_NewObject(ctx: *mut JSContext) -> JSValue;
    pub fn JS_GetGlobalObject(ctx: *mut JSContext) -> JSValue;
    pub fn JS_SetPropertyStr(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: *const c_char,
        val: JSValue,
    ) -> c_int;

    pub fn JS_SetContextOpaque(ctx: *mut JSContext, opaque: *mut c_void);
    pub fn JS_GetContextOpaque(ctx: *mut JSContext) -> *mut c_void;

    pub fn JS_DupValue(ctx: *mut JSContext, v: JSValue) -> JSValue;

    /// Returns a pointer into `obj`'s backing buffer (borrowed - do not
    /// free) and its byte length via `psize`. Null if `obj` isn't a
    /// Uint8Array (may also raise a JS exception on the context; not
    /// checked by this binding, see `js-runtime`'s crypto module).
    pub fn JS_GetUint8Array(ctx: *mut JSContext, psize: *mut usize, obj: JSValue) -> *mut u8;
}

/// `JS_NULL` — the `JS_MKVAL(JS_TAG_NULL, 0)` constant from quickjs.h.
pub const fn js_null() -> JSValue {
    JSValue {
        u: JSValueUnion { int32: 0 },
        tag: JS_TAG_NULL,
    }
}

/// `JS_UNDEFINED` — the `JS_MKVAL(JS_TAG_UNDEFINED, 0)` constant from quickjs.h.
pub const fn js_undefined() -> JSValue {
    JSValue {
        u: JSValueUnion { int32: 0 },
        tag: JS_TAG_UNDEFINED,
    }
}

/// Mirrors the `JS_NewBool` static inline from quickjs.h (`JS_MKVAL(JS_TAG_BOOL, val)`).
/// Doesn't need a `JSContext` — the C signature only takes one for API
/// consistency with the rest of `JS_New*`, the tag/value encoding itself
/// doesn't touch the runtime.
pub const fn js_bool(val: bool) -> JSValue {
    JSValue {
        u: JSValueUnion { int32: val as i32 },
        tag: JS_TAG_BOOL,
    }
}

/// Mirrors the non-NAN-boxed `__JS_NewFloat64` from quickjs.h: a plain
/// `{tag: JS_TAG_FLOAT64, u.float64: d}` value, no normalization needed
/// outside the NAN-boxed 32-bit encoding this binding doesn't support.
pub const fn js_float64(d: f64) -> JSValue {
    JSValue {
        u: JSValueUnion { float64: d },
        tag: JS_TAG_FLOAT64,
    }
}

/// Safe-ish helper mirroring the `JS_IsException` static inline from
/// quickjs.h (tag comparison only, no ownership implications).
pub fn js_is_exception(v: &JSValue) -> bool {
    v.tag == JS_TAG_EXCEPTION
}
