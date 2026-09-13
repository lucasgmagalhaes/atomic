//! `JSValue`/tag/calling-convention/class-related type and constant
//! definitions — split out from `lib.rs`.

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

pub const JS_TAG_STRING: i64 = -7;
pub const JS_TAG_OBJECT: i64 = -1;
pub const JS_TAG_INT: i64 = 0;
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
/// `JSCFunctionEnum::JS_CFUNC_generic_magic` — same calling shape as
/// `JS_CFUNC_GENERIC` plus a trailing `magic: c_int` (the value passed as
/// `JS_NewCFunction2`'s own `magic` argument), letting one native function
/// implementation serve several JS-visible functions that only differ by
/// which of several targets they act on (e.g. `localStorage` vs
/// `sessionStorage` sharing one `getItem`).
pub const JS_CFUNC_GENERIC_MAGIC: c_int = 1;

/// Matches `JSCFunction` from quickjs.h: the signature every function
/// registered via `JS_NewCFunction2` with `JS_CFUNC_GENERIC` must have.
pub type JSCFunction = unsafe extern "C" fn(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue;

/// `JSCFunctionEnum::JS_CFUNC_getter` — quickjs.c's `js_call_c_function`
/// dispatches this cproto to a 2-arg `(ctx, this_val) -> JSValue` call
/// through a `JSCFunctionType` union member, not the 4-arg `JSCFunction`
/// shape. Callers register a function with this narrower signature and
/// hand it to `JS_NewCFunction2` via `mem::transmute` to `JSCFunction` —
/// mirroring the same union punning quickjs.h itself does internally.
/// `JSCFunctionEnum::JS_CFUNC_constructor_or_func` - callable both as
/// `Foo()` and `new Foo()`; the native function receives `new.target`
/// (or `undefined` for a plain call) in place of `this_val` and is fully
/// responsible for building and returning the new object (quickjs
/// doesn't pre-allocate one, unlike a real JS `class` constructor).
pub const JS_CFUNC_CONSTRUCTOR_OR_FUNC: c_int = 4;
pub const JS_CFUNC_GETTER: c_int = 8;
/// `JSCFunctionEnum::JS_CFUNC_getter_magic` — a `(ctx, this_val, magic) ->
/// JSValue` getter, same magic-sharing idea as [`JS_CFUNC_GENERIC_MAGIC`].
pub const JS_CFUNC_GETTER_MAGIC: c_int = 10;
/// `JSCFunctionEnum::JS_CFUNC_setter` — see [`JS_CFUNC_GETTER`]; dispatches
/// to `(ctx, this_val, value) -> JSValue`.
pub const JS_CFUNC_SETTER: c_int = 9;
/// `JSCFunctionEnum::JS_CFUNC_setter_magic` — a `(ctx, this_val, value,
/// magic) -> JSValue` setter, same magic-sharing idea as
/// [`JS_CFUNC_GETTER_MAGIC`]/[`JS_CFUNC_GENERIC_MAGIC`] (one native
/// implementation serving several JS-visible accessors that only differ by
/// which property they act on, e.g. `element.style`'s per-property
/// setters).
pub const JS_CFUNC_SETTER_MAGIC: c_int = 11;

pub const JS_PROP_CONFIGURABLE: c_int = 1 << 0;
pub const JS_PROP_ENUMERABLE: c_int = 1 << 2;
pub const JS_PROP_HAS_GET: c_int = 1 << 11;
pub const JS_PROP_HAS_SET: c_int = 1 << 12;

pub type JSClassID = u32;
pub type JSAtom = u32;

/// Matches `JSClassFinalizer` from quickjs.h: called when the GC collects
/// an object of a custom class, so it can free the native data stashed via
/// `JS_SetOpaque`.
pub type JSClassFinalizer = unsafe extern "C" fn(rt: *mut JSRuntime, val: JSValue);

/// Mirrors `JSClassDef` from quickjs.h. `gc_mark`/`call`/`exotic` are
/// nullable function-pointer-shaped fields this workspace has no use for
/// yet (no cycles to mark, not a callable object, no exotic property
/// hooks) — kept as `*mut c_void` rather than typed callbacks since they're
/// always null here.
#[repr(C)]
pub struct JSClassDef {
    pub class_name: *const c_char,
    pub finalizer: Option<JSClassFinalizer>,
    pub gc_mark: *mut c_void,
    pub call: *mut c_void,
    pub exotic: *mut c_void,
}

#[repr(C)]
pub struct JSRuntime {
    _private: [u8; 0],
}

#[repr(C)]
pub struct JSContext {
    _private: [u8; 0],
}

/// Mirrors quickjs.h's `JSPropertyEnum` struct.
#[repr(C)]
pub struct JSPropertyEnum {
    pub is_enumerable: bool,
    pub atom: JSAtom,
}

/// `JS_GPN_STRING_MASK | JS_GPN_ENUM_ONLY` from quickjs.h - own,
/// enumerable, string-keyed properties only (no symbols, no private
/// fields, no inherited properties) - matches what a real structured
/// clone of a plain JS object would walk.
pub const JS_GPN_STRING_ENUM: c_int = (1 << 0) | (1 << 4);
