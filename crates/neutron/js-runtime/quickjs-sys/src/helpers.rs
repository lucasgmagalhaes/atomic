//! Constant-value helper constructors (`JS_NULL`/`JS_UNDEFINED`/...) and
//! `js_is_exception` — split out from `lib.rs`.

use crate::types::{
    JSValue, JSValueUnion, JS_TAG_BOOL, JS_TAG_EXCEPTION, JS_TAG_FLOAT64, JS_TAG_NULL,
    JS_TAG_UNDEFINED,
};

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

/// `JS_EXCEPTION` — return this from a native binding after an exception was
/// placed on the context with `JS_Throw`.
pub const fn js_exception() -> JSValue {
    JSValue {
        u: JSValueUnion { int32: 0 },
        tag: JS_TAG_EXCEPTION,
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
