//! `selectionStart`/`selectionEnd`/`selectionDirection`/
//! `setSelectionRange` on `Node.prototype` — split out from
//! `content/mod.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::js_helpers::define_getter_setter;

use super::super::node_registry::{dom_opaque, node_id};
use super::super::util::{new_js_string, read_js_string};

/// Reads a JS number value already tagged `INT`/`FLOAT64` as a
/// non-negative `usize` — no string/object-to-number coercion (no
/// `JS_ToFloat64` binding exists yet, same scope cut
/// `event_subclasses.rs::read_number` already documents).
unsafe fn read_number_value(val: sys::JSValue) -> Option<usize> {
    match val.tag {
        sys::JS_TAG_INT => Some(val.u.int32.max(0) as usize),
        sys::JS_TAG_FLOAT64 => Some(val.u.float64.max(0.0) as usize),
        _ => None,
    }
}

/// Reads argument `index` as a JS number (see [`read_number_value`]).
/// Absent/non-numeric arguments read as `None`, which callers use as
/// "keep the existing value" rather than throwing — matches a real
/// `setSelectionRange` tolerating a missing `direction` argument.
unsafe fn read_number_arg(argc: c_int, argv: *mut sys::JSValue, index: isize) -> Option<usize> {
    if (index as c_int) >= argc {
        return None;
    }
    read_number_value(*argv.offset(index))
}

unsafe extern "C" fn node_selection_start_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::JSValue {
            u: sys::JSValueUnion { int32: 0 },
            tag: sys::JS_TAG_INT,
        };
    };
    let dom_ptr = dom_opaque(ctx);
    let start = if dom_ptr.is_null() {
        0
    } else {
        (*dom_ptr).selection_range(id).0
    };
    sys::JSValue {
        u: sys::JSValueUnion {
            int32: start as i32,
        },
        tag: sys::JS_TAG_INT,
    }
}

unsafe extern "C" fn node_selection_end_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::JSValue {
            u: sys::JSValueUnion { int32: 0 },
            tag: sys::JS_TAG_INT,
        };
    };
    let dom_ptr = dom_opaque(ctx);
    let end = if dom_ptr.is_null() {
        0
    } else {
        (*dom_ptr).selection_range(id).1
    };
    sys::JSValue {
        u: sys::JSValueUnion { int32: end as i32 },
        tag: sys::JS_TAG_INT,
    }
}

unsafe extern "C" fn node_selection_direction_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return new_js_string(ctx, "none");
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return new_js_string(ctx, "none");
    }
    new_js_string(ctx, &(*dom_ptr).selection_range(id).2)
}

unsafe extern "C" fn node_selection_start_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom_ptr = dom_opaque(ctx);
    if !dom_ptr.is_null() {
        if let Some(start) = read_number_value(val) {
            let (_, end, direction) = (*dom_ptr).selection_range(id);
            // Per spec, the `selectionStart` setter (unlike
            // `setSelectionRange`) widens `end` rather than collapsing the
            // range when the new start moves past the current end.
            (*dom_ptr).set_selection_range(id, start, end.max(start), &direction);
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn node_selection_end_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom_ptr = dom_opaque(ctx);
    if !dom_ptr.is_null() {
        if let Some(end) = read_number_value(val) {
            let (start, _, direction) = (*dom_ptr).selection_range(id);
            // Mirrors `node_selection_start_set`'s own note: the
            // `selectionEnd` setter narrows `start` down rather than
            // collapsing when the new end moves before the current start.
            (*dom_ptr).set_selection_range(id, start.min(end), end, &direction);
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn node_selection_direction_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let Some(direction) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    let dom_ptr = dom_opaque(ctx);
    if !dom_ptr.is_null() {
        let (start, end, _) = (*dom_ptr).selection_range(id);
        (*dom_ptr).set_selection_range(id, start, end, &direction);
    }
    sys::js_undefined()
}

unsafe extern "C" fn node_set_selection_range(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::js_undefined();
    }
    let start = read_number_arg(argc, argv, 0).unwrap_or(0);
    let end = read_number_arg(argc, argv, 1).unwrap_or(start);
    let direction = if argc > 2 {
        read_js_string(ctx, *argv.offset(2)).unwrap_or_else(|| "none".to_string())
    } else {
        "none".to_string()
    };
    (*dom_ptr).set_selection_range(id, start, end, &direction);
    sys::js_undefined()
}

/// Defines the real `selectionStart`/`selectionEnd`/`selectionDirection`
/// accessors and the `setSelectionRange(start, end, direction)` method on
/// `proto` — backed by `dom::Dom::selection_range`/`set_selection_range`
/// (see that method's own doc for the clamping/normalization rules).
/// Present on every `Node` for the same "one generic class, not a typed
/// `HTMLInputElement`/`HTMLTextAreaElement` hierarchy" reason
/// `value`/`checked` already document.
pub(in super::super) unsafe fn define_selection_properties(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
) {
    define_getter_setter(
        ctx,
        proto,
        "selectionStart",
        node_selection_start_get,
        node_selection_start_set,
    );
    define_getter_setter(
        ctx,
        proto,
        "selectionEnd",
        node_selection_end_get,
        node_selection_end_set,
    );
    define_getter_setter(
        ctx,
        proto,
        "selectionDirection",
        node_selection_direction_get,
        node_selection_direction_set,
    );
    let name = std::ffi::CString::new("setSelectionRange").unwrap();
    let func = sys::JS_NewCFunction2(
        ctx,
        node_set_selection_range,
        name.as_ptr(),
        3,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, proto, name.as_ptr(), func);
}
