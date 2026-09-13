//! Raw `extern "C"` QuickJS-ng function declarations — split out from
//! `lib.rs`.

use std::os::raw::{c_char, c_int, c_void};

use crate::types::{
    JSAtom, JSCFunction, JSClassDef, JSClassID, JSContext, JSModuleDef, JSModuleLoaderFunc,
    JSModuleNormalizeFunc, JSPropertyEnum, JSRuntime, JSValue,
};

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

    pub fn JS_ParseJSON(
        ctx: *mut JSContext,
        buf: *const c_char,
        buf_len: usize,
        filename: *const c_char,
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

    /// Returns an owned reference (per the usual `JSValue`-return
    /// convention — caller must `JS_FreeValue` it): the JSON text as a
    /// string on success, `JS_UNDEFINED` when the value isn't
    /// representable (functions, `undefined`, cycles), or an exception
    /// marker when `toJSON` itself threw.
    pub fn JS_JSONStringify(
        ctx: *mut JSContext,
        obj: JSValue,
        replacer: JSValue,
        space: JSValue,
    ) -> JSValue;

    /// Runtime-wide allocation cap (bytes). Once exceeded, allocations
    /// from inside running script throw instead of succeeding — see
    /// quickjs-ng's `JS_SetMemoryLimit`.
    pub fn JS_SetMemoryLimit(rt: *mut JSRuntime, limit: usize);

    /// Called by the interpreter at loop/function-entry boundaries while
    /// script runs; return non-zero to abort execution with an
    /// "interrupted" InternalError. `opaque` is whatever was passed to
    /// [`JS_SetInterruptHandler`].
    pub fn JS_SetInterruptHandler(
        rt: *mut JSRuntime,
        cb: Option<unsafe extern "C" fn(rt: *mut JSRuntime, opaque: *mut c_void) -> c_int>,
        opaque: *mut c_void,
    );

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
    /// Returns the per-context prototype previously set via
    /// `JS_SetClassProto` for `class_id`. Returns a new reference.
    pub fn JS_GetClassProto(ctx: *mut JSContext, class_id: JSClassID) -> JSValue;

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

    /// Real Transferable-object support (`ROADMAP.md` item 32, scoped to
    /// `Uint8Array`): detaches `obj`'s backing `ArrayBuffer` - after this,
    /// every view onto it (this crate only ever builds/detects a plain
    /// `Uint8Array` one) observably has `byteLength === 0`, matching a
    /// real transferred buffer's spec behavior.
    pub fn JS_DetachArrayBuffer(ctx: *mut JSContext, obj: JSValue);

    /// Returns the real `ArrayBuffer` backing a typed array view (`obj`) -
    /// a new reference the caller must eventually free. Writes the view's
    /// own byte offset/length/element size into the three out-params if
    /// non-null (not used by this crate - always passed null).
    pub fn JS_GetTypedArrayBuffer(
        ctx: *mut JSContext,
        obj: JSValue,
        pbyte_offset: *mut usize,
        pbyte_length: *mut usize,
        pbytes_per_element: *mut usize,
    ) -> JSValue;

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

    /// Registers the runtime-wide module normalize/load callback pair
    /// (`ROADMAP.md` item 19) — both static `import` and dynamic
    /// `import()` route through these (see `JSModuleLoaderFunc`'s own
    /// doc). `module_normalize`/`opaque` may be null (a default identity
    /// normalizer); this crate always supplies its own.
    pub fn JS_SetModuleLoaderFunc(
        rt: *mut JSRuntime,
        module_normalize: Option<JSModuleNormalizeFunc>,
        module_loader: Option<JSModuleLoaderFunc>,
        opaque: *mut c_void,
    );

    /// Links `module` (already-compiled, from `JS_Eval(...,
    /// JS_EVAL_TYPE_MODULE | JS_EVAL_FLAG_COMPILE_ONLY)`) and its whole
    /// static-import graph, invoking the registered `JSModuleLoaderFunc`
    /// once per not-yet-loaded specifier. Returns `< 0` on failure (an
    /// exception is left pending on `ctx`).
    pub fn JS_ResolveModule(ctx: *mut JSContext, obj: JSValue) -> c_int;

    /// Executes a linked module's top-level code (must run after a
    /// successful [`JS_ResolveModule`]) — the second half of quickjs.h's
    /// documented compile/link/execute module lifecycle. Consumes `func_obj`.
    pub fn JS_EvalFunction(ctx: *mut JSContext, func_obj: JSValue) -> JSValue;

    /// The module's real namespace object (its exported bindings as
    /// enumerable properties) — only meaningful after `m` has been
    /// resolved/evaluated. Returns a new reference.
    pub fn JS_GetModuleNamespace(ctx: *mut JSContext, m: *mut JSModuleDef) -> JSValue;

    /// Allocates a copy of `str` using quickjs-ng's own allocator — the
    /// only safe way to build the string a [`JSModuleNormalizeFunc`]
    /// returns, since quickjs-ng frees it with `js_free`, not the system
    /// allocator `CString`/`libc::malloc` would use.
    pub fn js_strdup(ctx: *mut JSContext, str1: *const c_char) -> *mut c_char;

    pub fn JS_ThrowReferenceError(ctx: *mut JSContext, fmt: *const c_char, ...) -> JSValue;
}
