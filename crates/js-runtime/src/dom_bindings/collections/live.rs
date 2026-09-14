//! Real *live* `HTMLCollection`: `getElementsByTagName`/
//! `getElementsByClassName` re-run their query on every access (`.length`,
//! indexed access, `item()`, `namedItem()`) rather than snapshotting once
//! at call time - `html_collection.rs`'s `html_collection` stays the
//! snapshot-array shape used for `querySelectorAll`/`document.forms`/
//! `images`/`links`/`scripts`/`childNodes` etc (still real per those own
//! docs, just not live - a narrower, separate scope cut this file doesn't
//! touch).
//!
//! Built on a real ES2015 `Proxy` (confirmed working in this engine's
//! QuickJS-ng build - `JS_NewProxy`, `quickjs-sys/src/ffi.rs`), not a raw
//! `JSClassExoticMethods` hook (no class in this codebase uses one; every
//! `JSClassDef::exotic` is `null`) - a `get`/`has` trap pair implemented
//! as two small native C functions is a much smaller, already-proven
//! surface (this crate already builds native `JSCFunction`s everywhere)
//! than hand-writing the lower-level exotic-property-hook contract from
//! scratch as this engine's first use of it.
//!
//! Scope cut: only `get`/`has` traps are installed - no `ownKeys`/
//! `getOwnPropertyDescriptor`. `Object.keys()`/`for...in`/spread on a
//! live collection fall through to the (empty) target object rather than
//! enumerating - real Proxy invariants require `getOwnPropertyDescriptor`
//! to agree with anything `ownKeys` reports, so omitting both together is
//! the safe, non-invariant-violating way to leave enumeration
//! unsupported rather than partially/incorrectly supported.
//! `Array.from(liveCollection)` still works despite this: with no
//! `Symbol.iterator` trap either, `Array.from` falls back to its real
//! array-like path (`.length` + indexed `get`), which this file does
//! support.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::super::node_registry::{dom_opaque, node_class_id_for, node_object};
use super::super::selectors::{matching_by_class, matching_by_tag};
use super::super::util::read_js_string;

const LIVE_COLLECTION_CLASS_KIND: &str = "LiveHTMLCollection";

/// What a live collection re-queries on every access - the exact
/// `(start, ..., include_start)` shape `matching_by_tag`/
/// `matching_by_class` already take, just re-run instead of snapshotted
/// once.
enum Query {
    Tag(String),
    Class(String),
}

struct LiveCollectionState {
    root: dom::NodeId,
    query: Query,
    include_start: bool,
}

unsafe extern "C" fn finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = sys::JS_GetOpaque(
        val,
        crate::class_registry::class_id_for(rt, LIVE_COLLECTION_CLASS_KIND),
    ) as *mut LiveCollectionState;
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

unsafe fn ensure_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("LiveHTMLCollection").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    crate::class_registry::ensure_class(rt, LIVE_COLLECTION_CLASS_KIND, &def)
}

/// Re-runs the real query a live collection's `target` was built with -
/// always `Some` (possibly empty) once past the null-opaque guard; the
/// only real failure mode is no `dom::Dom` to query, which also
/// degrades to empty rather than a thrown error (matches every other
/// "no DOM" degrade-to-empty convention this crate's DOM bindings use).
unsafe fn resolve(ctx: *mut sys::JSContext, target: sys::JSValue) -> Vec<dom::NodeId> {
    let class_id =
        crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), LIVE_COLLECTION_CLASS_KIND);
    let state = sys::JS_GetOpaque(target, class_id) as *mut LiveCollectionState;
    if state.is_null() {
        return Vec::new();
    }
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return Vec::new();
    }
    let include_start = (*state).include_start;
    let result = match &(*state).query {
        Query::Tag(tag) => matching_by_tag(&*dom_ptr, (*state).root, tag, include_start),
        Query::Class(class_name) => {
            matching_by_class(&*dom_ptr, (*state).root, class_name, include_start)
        }
    };
    result.unwrap_or_default()
}

/// `id`/`name` match, same rule `html_collection.rs`'s own `namedItem`
/// already implements for the snapshot shape - kept independent rather
/// than shared since that one reads back through already-built JS
/// element objects and this one reads `dom::Dom` attributes directly
/// (cheaper - no need to materialize every candidate as a JS object just
/// to check its `id`/`name`).
unsafe fn named_match(dom: &dom::Dom, node: dom::NodeId, name: &str) -> bool {
    dom.attribute(node, "id") == Some(name) || dom.attribute(node, "name") == Some(name)
}

/// Reads a JS number value already tagged `INT`/`FLOAT64` as a
/// non-negative `usize` index - no string/object-to-number coercion (no
/// `JS_ToFloat64`/`JS_ToInt32` binding exists yet, same scope cut
/// `content/selection.rs`'s own `read_number_value` already documents).
unsafe fn read_index(val: sys::JSValue) -> Option<usize> {
    match val.tag {
        sys::JS_TAG_INT if val.u.int32 >= 0 => Some(val.u.int32 as usize),
        sys::JS_TAG_FLOAT64 if val.u.float64 >= 0.0 => Some(val.u.float64 as usize),
        _ => None,
    }
}

/// Builds a real `JS_TAG_INT` `JSValue` - no `sys::js_int32` helper
/// exists yet (only `js_bool`/`js_undefined`/etc), same manual
/// `JSValue { u: JSValueUnion { int32: ... }, tag: JS_TAG_INT }`
/// construction `content/selection.rs` already uses.
unsafe fn int_value(n: i32) -> sys::JSValue {
    sys::JSValue {
        u: sys::JSValueUnion { int32: n },
        tag: sys::JS_TAG_INT,
    }
}

unsafe extern "C" fn item_method(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let target = sys::JS_GetProxyTarget(ctx, this_val);
    let nodes = resolve(ctx, target);
    sys::JS_FreeValue(ctx, target);
    if argc < 1 {
        return sys::js_null();
    }
    let Some(index) = read_index(*argv) else {
        return sys::js_null();
    };
    match nodes.get(index) {
        Some(&id) => {
            let dom_ptr = dom_opaque(ctx);
            let class_id = node_class_id_for(ctx, dom_ptr, id);
            node_object(ctx, class_id, id)
        }
        None => sys::js_null(),
    }
}

unsafe extern "C" fn named_item_method(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let target = sys::JS_GetProxyTarget(ctx, this_val);
    let nodes = resolve(ctx, target);
    sys::JS_FreeValue(ctx, target);
    if argc < 1 {
        return sys::js_null();
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return sys::js_null();
    };
    if name.is_empty() {
        return sys::js_null();
    }
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::js_null();
    }
    for &id in &nodes {
        if named_match(&*dom_ptr, id, &name) {
            let class_id = node_class_id_for(ctx, dom_ptr, id);
            return node_object(ctx, class_id, id);
        }
    }
    sys::js_null()
}

/// Real `Proxy` `get(target, property, receiver)` trap - `argv[0]` is
/// `target`, `argv[1]` the property key (read as a string; a `Symbol`
/// key reads back `None` from `read_js_string` and falls through to
/// `undefined`, same scope cut every other ad-hoc `read_*` helper in
/// this crate already documents for missing `JS_ToCStringLen2`-based
/// symbol support).
unsafe extern "C" fn get_trap(
    ctx: *mut sys::JSContext,
    _handler_this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return sys::js_undefined();
    }
    let target = *argv;
    let Some(prop) = read_js_string(ctx, *argv.add(1)) else {
        return sys::js_undefined();
    };
    if prop == "length" {
        return int_value(resolve(ctx, target).len() as i32);
    }
    if prop == "item" {
        let name = CString::new("item").unwrap();
        return sys::JS_NewCFunction2(ctx, item_method, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
    }
    if prop == "namedItem" {
        let name = CString::new("namedItem").unwrap();
        return sys::JS_NewCFunction2(
            ctx,
            named_item_method,
            name.as_ptr(),
            1,
            sys::JS_CFUNC_GENERIC,
            0,
        );
    }
    if let Ok(idx) = prop.parse::<usize>() {
        return match resolve(ctx, target).get(idx) {
            Some(&id) => {
                let dom_ptr = dom_opaque(ctx);
                let class_id = node_class_id_for(ctx, dom_ptr, id);
                node_object(ctx, class_id, id)
            }
            None => sys::js_undefined(),
        };
    }
    sys::js_undefined()
}

/// Real `Proxy` `has(target, property)` trap - backs `in`/implicit
/// existence checks for `"length"`/`"item"`/`"namedItem"`/a valid
/// numeric index.
unsafe extern "C" fn has_trap(
    ctx: *mut sys::JSContext,
    _handler_this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return sys::js_bool(false);
    }
    let target = *argv;
    let Some(prop) = read_js_string(ctx, *argv.add(1)) else {
        return sys::js_bool(false);
    };
    if prop == "length" || prop == "item" || prop == "namedItem" {
        return sys::js_bool(true);
    }
    if let Ok(idx) = prop.parse::<usize>() {
        return sys::js_bool(idx < resolve(ctx, target).len());
    }
    sys::js_bool(false)
}

unsafe fn build(
    ctx: *mut sys::JSContext,
    root: dom::NodeId,
    query: Query,
    include_start: bool,
) -> sys::JSValue {
    let class_id = ensure_class(ctx);
    let target = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&target) {
        return target;
    }
    let state = Box::new(LiveCollectionState {
        root,
        query,
        include_start,
    });
    sys::JS_SetOpaque(target, Box::into_raw(state) as *mut std::ffi::c_void);

    let handler = sys::JS_NewObject(ctx);
    let get_name = CString::new("get").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        handler,
        get_name.as_ptr(),
        sys::JS_NewCFunction2(
            ctx,
            get_trap,
            get_name.as_ptr(),
            3,
            sys::JS_CFUNC_GENERIC,
            0,
        ),
    );
    let has_name = CString::new("has").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        handler,
        has_name.as_ptr(),
        sys::JS_NewCFunction2(
            ctx,
            has_trap,
            has_name.as_ptr(),
            2,
            sys::JS_CFUNC_GENERIC,
            0,
        ),
    );

    let proxy = sys::JS_NewProxy(ctx, target, handler);
    sys::JS_FreeValue(ctx, target);
    sys::JS_FreeValue(ctx, handler);
    proxy
}

pub(in super::super) unsafe fn live_elements_by_tag_name(
    ctx: *mut sys::JSContext,
    root: dom::NodeId,
    tag: &str,
    include_start: bool,
) -> sys::JSValue {
    build(ctx, root, Query::Tag(tag.to_string()), include_start)
}

pub(in super::super) unsafe fn live_elements_by_class_name(
    ctx: *mut sys::JSContext,
    root: dom::NodeId,
    class_name: &str,
    include_start: bool,
) -> sys::JSValue {
    build(
        ctx,
        root,
        Query::Class(class_name.to_string()),
        include_start,
    )
}
