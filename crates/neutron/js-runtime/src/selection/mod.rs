//! Real `Range`/`Selection` (`ROADMAP.md` item 20, "text selection") —
//! the one remaining sub-scope of Default Actions (button/input submit/
//! reset, `<a href>` click navigation, and Tab/Shift+Tab focus navigation
//! are all already real).
//!
//! Neither class stores native/opaque state — both are plain
//! `JS_NewObject` instances with a real prototype (`resolve_prototype`,
//! same convention `abort_controller`'s own `AbortController`/
//! `AbortSignal` already established), carrying hidden `__`-prefixed own
//! properties for state (a real `Node` object for each boundary point,
//! `js_float64` for each offset — same "state as plain JS values on the
//! object" convention `history.rs`'s `__entries`/`__index` already use).
//!
//! `Range` (`range.rs`) is real for **text-node boundary points only** —
//! `setStart`/`setEnd`/`selectNodeContents` validate the given node is a
//! real `dom::NodeData::Text` node before storing it; an element-node
//! boundary (real spec's child-index-based offset) is a documented no-op,
//! matching this crate's existing "real but narrower, no-op on
//! out-of-scope input" convention (e.g. `crypto.getRandomValues`'s
//! `Uint8Array`-only scope). `toString()` is real cross-node text
//! concatenation: a pre-order walk of every real text node in the whole
//! document, sliced at the range's two boundary points.
//!
//! `Selection` (`selection_obj.rs`) holds at most one real `Range` at a
//! time (real modern-browser behavior — `rangeCount` is `0` or `1`, not
//! the legacy multi-range model some engines still expose).
//! `window.getSelection()`/`document.getSelection()` (wired in
//! `window.rs`/`dom_bindings/document.rs`) both return the *same* object
//! every call (real spec identity) via this module's own per-context
//! singleton cache (`get_or_create`), the same shape
//! `window_registry::REMOTE_WINDOW_OBJECTS` already established for
//! `iframe.contentWindow`'s own identity requirement.
//!
//! Real, documented scope cut: **no user-driven mouse-drag/keyboard text
//! selection** — only the script-driven API surface (`document.createRange()`,
//! `getSelection()`, and everything callable on the objects they return)
//! is real here. A real drag-to-select interaction loop would need a new
//! host input command in `profile-worker` — separate, larger follow-up
//! work, not attempted in this pass.

use std::ffi::CString;

use quickjs_sys as sys;

mod helpers;
mod range;
mod selection_obj;

pub(crate) use selection_obj::{cleanup, get_or_create};

use range::{
    make_range, range_collapse, range_collapsed_get, range_constructor, range_end_container_get,
    range_end_offset_get, range_select_node_contents, range_set_end, range_set_start,
    range_start_container_get, range_start_offset_get, range_to_string,
};
use selection_obj::{
    selection_add_range, selection_anchor_node_get, selection_anchor_offset_get,
    selection_collapse, selection_focus_node_get, selection_focus_offset_get,
    selection_get_range_at, selection_range_count_get, selection_remove_all_ranges,
    selection_to_string,
};

/// `document.createRange()` — real, shares `Range`'s own construction
/// with `new Range()` (`range::make_range`).
pub(crate) unsafe extern "C" fn document_create_range(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    make_range(ctx, sys::js_undefined())
}

/// `window.getSelection()`/`document.getSelection()` — real, both call
/// this same entry point so the returned object's identity matches.
pub(crate) unsafe extern "C" fn get_selection(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    get_or_create(ctx)
}

/// Registers the global `Range`/`Selection` constructors.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);

    let range_proto = sys::JS_NewObject(ctx);
    crate::js_helpers::define_getter(
        ctx,
        range_proto,
        "startContainer",
        range_start_container_get,
    );
    crate::js_helpers::define_getter(ctx, range_proto, "endContainer", range_end_container_get);
    crate::js_helpers::define_getter(ctx, range_proto, "startOffset", range_start_offset_get);
    crate::js_helpers::define_getter(ctx, range_proto, "endOffset", range_end_offset_get);
    crate::js_helpers::define_getter(ctx, range_proto, "collapsed", range_collapsed_get);
    crate::js_helpers::define_method(ctx, range_proto, "setStart", range_set_start, 2);
    crate::js_helpers::define_method(ctx, range_proto, "setEnd", range_set_end, 2);
    crate::js_helpers::define_method(
        ctx,
        range_proto,
        "selectNodeContents",
        range_select_node_contents,
        1,
    );
    crate::js_helpers::define_method(ctx, range_proto, "collapse", range_collapse, 0);
    crate::js_helpers::define_method(ctx, range_proto, "toString", range_to_string, 0);

    let range_ctor_name = CString::new("Range").unwrap();
    let range_ctor = sys::JS_NewCFunction2(
        ctx,
        range_constructor,
        range_ctor_name.as_ptr(),
        0,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(ctx, range_ctor, proto_name.as_ptr(), range_proto);
    sys::JS_SetPropertyStr(ctx, global, range_ctor_name.as_ptr(), range_ctor);

    let selection_proto = sys::JS_NewObject(ctx);
    crate::js_helpers::define_getter(
        ctx,
        selection_proto,
        "rangeCount",
        selection_range_count_get,
    );
    crate::js_helpers::define_getter(
        ctx,
        selection_proto,
        "anchorNode",
        selection_anchor_node_get,
    );
    crate::js_helpers::define_getter(
        ctx,
        selection_proto,
        "anchorOffset",
        selection_anchor_offset_get,
    );
    crate::js_helpers::define_getter(ctx, selection_proto, "focusNode", selection_focus_node_get);
    crate::js_helpers::define_getter(
        ctx,
        selection_proto,
        "focusOffset",
        selection_focus_offset_get,
    );
    crate::js_helpers::define_method(ctx, selection_proto, "addRange", selection_add_range, 1);
    crate::js_helpers::define_method(
        ctx,
        selection_proto,
        "removeAllRanges",
        selection_remove_all_ranges,
        0,
    );
    crate::js_helpers::define_method(
        ctx,
        selection_proto,
        "getRangeAt",
        selection_get_range_at,
        1,
    );
    crate::js_helpers::define_method(ctx, selection_proto, "collapse", selection_collapse, 1);
    crate::js_helpers::define_method(ctx, selection_proto, "toString", selection_to_string, 0);
    // `Selection` has no public constructor in the real spec either — only
    // `getSelection()` ever produces one. This module never registers a
    // global `Selection` constructor function, only its prototype, which
    // `resolve_prototype`'s fallback lookup (`globalThis.Selection.prototype`)
    // still needs to find - so a bare prototype-holder object is exposed
    // under that name instead of a real constructor.
    let selection_holder = sys::JS_NewObject(ctx);
    sys::JS_SetPropertyStr(ctx, selection_holder, proto_name.as_ptr(), selection_proto);
    let selection_name = CString::new("Selection").unwrap();
    sys::JS_SetPropertyStr(ctx, global, selection_name.as_ptr(), selection_holder);

    sys::JS_FreeValue(ctx, global);
}
