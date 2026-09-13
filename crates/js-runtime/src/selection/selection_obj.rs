//! `Selection` (`ROADMAP.md` item 20, "text selection") — real for
//! script-driven construction (`addRange`/`collapse`), one range at a
//! time (real modern-browser behavior, not the legacy multi-range
//! model). See `selection/mod.rs`'s own module doc for the full scope note.

use std::cell::RefCell;
use std::collections::HashMap;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::helpers::{get_prop, new_js_string, resolve_prototype, set_prop, AND_RANGE};
use super::range::{make_collapsed_range_at, range_to_string};

thread_local! {
    // One real `Selection` object per context - `window.getSelection()`
    // and `document.getSelection()` must return the *same* object every
    // call (real spec identity), same shape
    // `window_registry::REMOTE_WINDOW_OBJECTS` already established for
    // `iframe.contentWindow`'s own identity requirement.
    static SELECTIONS: RefCell<HashMap<usize, sys::JSValue>> = RefCell::new(HashMap::new());
}

/// Returns this context's one real `Selection` object, building it lazily
/// on first access. Every call after the first hands back a duped
/// reference to the same object.
pub(crate) unsafe fn get_or_create(ctx: *mut sys::JSContext) -> sys::JSValue {
    let ctx_key = ctx as usize;
    let cached = SELECTIONS.with(|reg| reg.borrow().get(&ctx_key).copied());
    if let Some(obj) = cached {
        return sys::JS_DupValue(ctx, obj);
    }

    let proto = resolve_prototype(ctx, sys::js_undefined(), "Selection");
    let obj = sys::JS_NewObject(ctx);
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);
    set_prop(ctx, obj, AND_RANGE, sys::js_undefined());

    SELECTIONS.with(|reg| {
        reg.borrow_mut().insert(ctx_key, sys::JS_DupValue(ctx, obj));
    });
    obj
}

/// Frees this context's cached `Selection` object - called from
/// `Context::drop`, same "one more `cleanup(ctx)` call" convention every
/// other per-context object cache in this crate already has.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    if let Some(obj) = SELECTIONS.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
        sys::JS_FreeValue(ctx, obj);
    }
}

unsafe fn stored_range(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue {
    get_prop(ctx, this_val, AND_RANGE)
}

pub(super) unsafe extern "C" fn selection_add_range(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc >= 1 {
        set_prop(ctx, this_val, AND_RANGE, sys::JS_DupValue(ctx, *argv));
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn selection_remove_all_ranges(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    set_prop(ctx, this_val, AND_RANGE, sys::js_undefined());
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn selection_get_range_at(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let index = if argc >= 1 {
        match (*argv).tag {
            sys::JS_TAG_INT => (*argv).u.int32,
            sys::JS_TAG_FLOAT64 => (*argv).u.float64 as i32,
            _ => -1,
        }
    } else {
        -1
    };
    let range = stored_range(ctx, this_val);
    if index == 0 && range.tag != sys::JS_TAG_UNDEFINED {
        return range;
    }
    sys::JS_FreeValue(ctx, range);
    // Out-of-range index: a documented no-op returning `undefined` rather
    // than throwing `IndexSizeError` - same convention `Range`'s own
    // non-text-node boundary handling already takes.
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn selection_range_count_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let range = stored_range(ctx, this_val);
    let count = if range.tag == sys::JS_TAG_UNDEFINED {
        0
    } else {
        1
    };
    sys::JS_FreeValue(ctx, range);
    sys::js_float64(count as f64)
}

pub(super) unsafe extern "C" fn selection_to_string(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let range = stored_range(ctx, this_val);
    if range.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, range);
        return new_js_string(ctx, "");
    }
    let result = range_to_string(ctx, range, argc, argv);
    sys::JS_FreeValue(ctx, range);
    result
}

pub(super) unsafe extern "C" fn selection_collapse(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        set_prop(ctx, this_val, AND_RANGE, sys::js_undefined());
        return sys::js_undefined();
    }
    let node = *argv;
    let offset = if argc >= 2 {
        match (*argv.add(1)).tag {
            sys::JS_TAG_INT => (*argv.add(1)).u.int32.max(0) as usize,
            sys::JS_TAG_FLOAT64 => (*argv.add(1)).u.float64.max(0.0) as usize,
            _ => 0,
        }
    } else {
        0
    };
    let range = make_collapsed_range_at(ctx, node, offset);
    set_prop(ctx, this_val, AND_RANGE, range);
    sys::js_undefined()
}

unsafe fn anchor_or_focus_node(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    key: &[u8],
) -> sys::JSValue {
    let range = stored_range(ctx, this_val);
    if range.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, range);
        return sys::js_null();
    }
    let value = get_prop(ctx, range, key);
    sys::JS_FreeValue(ctx, range);
    value
}

pub(super) unsafe extern "C" fn selection_anchor_node_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    anchor_or_focus_node(ctx, this_val, super::helpers::AND_START_NODE)
}
pub(super) unsafe extern "C" fn selection_focus_node_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    anchor_or_focus_node(ctx, this_val, super::helpers::AND_END_NODE)
}

unsafe fn anchor_or_focus_offset(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    key: &[u8],
) -> sys::JSValue {
    let range = stored_range(ctx, this_val);
    if range.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, range);
        return sys::js_float64(0.0);
    }
    let value = get_prop(ctx, range, key);
    sys::JS_FreeValue(ctx, range);
    value
}

pub(super) unsafe extern "C" fn selection_anchor_offset_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    anchor_or_focus_offset(ctx, this_val, super::helpers::AND_START_OFFSET)
}
pub(super) unsafe extern "C" fn selection_focus_offset_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    anchor_or_focus_offset(ctx, this_val, super::helpers::AND_END_OFFSET)
}
