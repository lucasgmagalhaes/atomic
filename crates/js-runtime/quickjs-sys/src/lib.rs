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

    /// Returns an owned reference (per the usual `JSValue`-return
    /// convention — caller must `JS_FreeValue` it), `JS_UNDEFINED` if the
    /// property doesn't exist.
    pub fn JS_GetPropertyStr(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: *const c_char,
    ) -> JSValue;

    /// `argv` may be null when `argc == 0`.
    pub fn JS_Call(
        ctx: *mut JSContext,
        func_obj: JSValue,
        this_obj: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;

    pub fn JS_IsFunction(ctx: *mut JSContext, val: JSValue) -> bool;
    pub fn JS_SetConstructorBit(ctx: *mut JSContext, func_obj: JSValue, val: bool) -> bool;
    pub fn JS_ToBool(ctx: *mut JSContext, val: JSValue) -> c_int;
    pub fn JS_GetException(ctx: *mut JSContext) -> JSValue;
    pub fn JS_HasException(ctx: *mut JSContext) -> bool;

    pub fn JS_GetRuntime(ctx: *mut JSContext) -> *mut JSRuntime;

    /// Allocates a class ID the first time `*pclass_id == 0` (writing it
    /// back), otherwise returns the existing value unchanged — safe to call
    /// repeatedly with the same backing storage across multiple runtimes.
    pub fn JS_NewClassID(rt: *mut JSRuntime, pclass_id: *mut JSClassID) -> JSClassID;
    /// Registers `class_def` under `class_id` on `rt`. Returns `< 0` if
    /// `class_id` is already registered on this particular runtime (e.g. a
    /// second `Context` sharing the same `Runtime`) — the class definition
    /// from the first call still stands, so callers can ignore the error.
    pub fn JS_NewClass(
        rt: *mut JSRuntime,
        class_id: JSClassID,
        class_def: *const JSClassDef,
    ) -> c_int;
    /// Creates an instance of `class_id` using the prototype most recently
    /// set via `JS_SetClassProto` for this context.
    pub fn JS_NewObjectClass(ctx: *mut JSContext, class_id: JSClassID) -> JSValue;
    /// Sets the per-context prototype used by `JS_NewObjectClass` for
    /// `class_id`. Takes ownership of `obj`.
    pub fn JS_SetClassProto(ctx: *mut JSContext, class_id: JSClassID, obj: JSValue);

    /// Only supported for custom classes (`class_id >= JS_CLASS_INIT_COUNT`,
    /// true for every ID `JS_NewClassID` hands out). Returns `< 0` if `obj`
    /// isn't an object of a registered custom class.
    pub fn JS_SetOpaque(obj: JSValue, opaque: *mut c_void) -> c_int;
    /// Returns null if `obj` isn't an object of exactly `class_id`.
    pub fn JS_GetOpaque(obj: JSValue, class_id: JSClassID) -> *mut c_void;

    pub fn JS_NewAtom(ctx: *mut JSContext, str1: *const c_char) -> JSAtom;
    pub fn JS_FreeAtom(ctx: *mut JSContext, v: JSAtom);

    /// Defines an accessor property. Takes ownership of both `getter` and
    /// `setter`.
    pub fn JS_DefinePropertyGetSet(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: JSAtom,
        getter: JSValue,
        setter: JSValue,
        flags: c_int,
    ) -> c_int;

    /// Returns a pointer into `obj`'s backing buffer (borrowed - do not
    /// free) and its byte length via `psize`. Null if `obj` isn't a
    /// Uint8Array (may also raise a JS exception on the context; not
    /// checked by this binding, see `js-runtime`'s crypto module).
    pub fn JS_GetUint8Array(ctx: *mut JSContext, psize: *mut usize, obj: JSValue) -> *mut u8;

    /// Copies `buf[..len]` into a freshly allocated `Uint8Array`/
    /// `ArrayBuffer` (unlike [`JS_GetUint8Array`], `buf` isn't borrowed
    /// afterward - safe to free/drop the Rust-side buffer once this
    /// returns).
    pub fn JS_NewUint8ArrayCopy(ctx: *mut JSContext, buf: *const u8, len: usize) -> JSValue;
    pub fn JS_NewArrayBufferCopy(ctx: *mut JSContext, buf: *const u8, len: usize) -> JSValue;

    pub fn JS_NewArray(ctx: *mut JSContext) -> JSValue;
    /// `true`/`false` reflect `Array.isArray`-shaped intent, but per
    /// quickjs-ng's own doc comment this no longer punches through
    /// proxies - fine for this crate's use (only ever called on values it
    /// itself constructed or received as plain arguments, never a proxy).
    pub fn JS_IsArray(val: JSValue) -> bool;
    pub fn JS_IsStrictEqual(ctx: *mut JSContext, op1: JSValue, op2: JSValue) -> bool;

    pub fn JS_GetProperty(ctx: *mut JSContext, this_obj: JSValue, prop: JSAtom) -> JSValue;
    pub fn JS_GetPropertyUint32(ctx: *mut JSContext, this_obj: JSValue, idx: u32) -> JSValue;
    /// Returns `< 0` on failure. Takes ownership of `val` (consistent with
    /// every other `JS_Set*` in this binding set).
    pub fn JS_SetPropertyUint32(
        ctx: *mut JSContext,
        this_obj: JSValue,
        idx: u32,
        val: JSValue,
    ) -> c_int;

    /// Writes `length` into `*pres`. Works for arrays and any other object
    /// with a numeric `.length` (array-likes) - used here only on real
    /// arrays.
    pub fn JS_GetLength(ctx: *mut JSContext, obj: JSValue, pres: *mut i64) -> c_int;

    /// Enumerates `obj`'s own properties matching `flags` into a
    /// heap-allocated array the caller must free with
    /// [`JS_FreePropertyEnum`]. Returns `< 0` on failure.
    pub fn JS_GetOwnPropertyNames(
        ctx: *mut JSContext,
        ptab: *mut *mut JSPropertyEnum,
        plen: *mut u32,
        obj: JSValue,
        flags: c_int,
    ) -> c_int;
    pub fn JS_FreePropertyEnum(ctx: *mut JSContext, tab: *mut JSPropertyEnum, len: u32);
    pub fn JS_AtomToCStringLen(
        ctx: *mut JSContext,
        plen: *mut usize,
        atom: JSAtom,
    ) -> *const c_char;

    /// Creates a `Promise` plus its `resolve`/`reject` functions, written
    /// into `resolving_funcs[0]`/`[1]` (must point at a 2-element array).
    /// Calling either function later queues the promise's reaction jobs -
    /// [`JS_ExecutePendingJob`] actually runs them.
    pub fn JS_NewPromiseCapability(ctx: *mut JSContext, resolving_funcs: *mut JSValue) -> JSValue;

    /// Runs one job off `rt`'s internal job queue (a `Promise` reaction, or
    /// microtask-equivalent) - the real quickjs mechanism `.then()`/
    /// `async`/`await` continuations run through. `*pctx` receives the
    /// context the job runs in (relevant for multi-context runtimes; this
    /// crate only ever has one `Context` per `Runtime`, so it's always the
    /// same). Returns `> 0` if a job ran, `0` if the queue was empty,
    /// `< 0` if the job itself threw (the exception is left pending on
    /// `*pctx`, same convention as any other failed `JS_Eval`/`JS_Call`).
    pub fn JS_ExecutePendingJob(rt: *mut JSRuntime, pctx: *mut *mut JSContext) -> c_int;

    pub fn JS_Throw(ctx: *mut JSContext, obj: JSValue) -> JSValue;

    /// Takes ownership of `proto_val`. Used by a native constructor
    /// (`JS_CFUNC_CONSTRUCTOR_OR_FUNC`) to link a freshly built plain
    /// object to its class's shared prototype, since quickjs doesn't
    /// pre-allocate/pre-link one the way it does for `class`-declared
    /// constructors.
    pub fn JS_SetPrototype(ctx: *mut JSContext, obj: JSValue, proto_val: JSValue) -> c_int;

    pub fn JS_IsJobPending(rt: *mut JSRuntime) -> bool;
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
