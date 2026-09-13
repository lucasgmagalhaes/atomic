//! `window` — `window` is simply an alias for the global object itself,
//! matching how a real top-level `window` *is* `globalThis` in its own
//! realm. `self`/`top`/`parent` all alias the same object too — correct
//! for every window this crate can create (`window.open()` only ever
//! makes a new *top-level* browsing context, never an embedded frame —
//! see `window_registry`'s own doc for why `iframe` isn't modeled), so a
//! window is always its own `top`/`parent` here, exactly like a real
//! top-level window/tab is.
//!
//! Real `window.open()`/`RemoteWindow` (`ROADMAP.md` items 36/37): opens
//! a genuinely independent second `JSContext` (own global object, own
//! blank `dom::Dom`) via `window_registry::open_window`, returning a
//! `RemoteWindow` — a plain object (same "no native quickjs class" shape
//! `abort_controller`/`message_channel` already use) carrying the new
//! window's id and real `postMessage(data)`/`close()` methods. The
//! opened window's own `opener` points back the same way. See
//! `window_registry`'s own module doc for the cross-`JSContext` message
//! delivery mechanism and the `iframe` scope cut.
//!
//! Also the real document-level scroll surface (`ROADMAP.md` P3 item 22):
//! `scrollY`/`pageYOffset` read, and `scroll`/`scrollTo`/`scrollBy` write,
//! `host_state::HostState::scroll_y` — the same value `profile-worker`'s
//! `SCROLL` command already drove into `Page::render`'s paint offset before
//! this, just not readable/writable from script until now. `scrollX`/
//! `pageXOffset` always read `0` — no horizontal scroll modeled, matching
//! `layout-engine`'s own scope. A real, synchronous `"scroll"` event fires
//! on `window` on every `scroll`/`scrollTo`/`scrollBy` call — this crate's
//! existing convention (`input`, `change`, `reset`, `submit`, ...) is
//! synchronous side-effect dispatch rather than queuing for a task-source
//! turn a real browser's scroll event technically uses; a script observing
//! it can't tell the difference without measuring real wall-clock delay.
//! The host, not this event, is what actually clamps the value against
//! real content height (`profile-worker`'s own `sync_scroll` reads it back
//! before every paint) — a page requesting an out-of-range offset here
//! just sees it corrected on the next read, same "host is authoritative,
//! script requests" split real compositor-driven scrolling has.
//!
//! Also real `innerWidth`/`innerHeight` (`ROADMAP.md` P3 item 23): one-way
//! reads of `host_state::HostState::viewport_width`/`viewport_height`, set
//! by `Context::set_viewport_size` whenever a host lays a page out against
//! a real size. No setter — real `window.innerWidth`/`innerHeight` are
//! spec-read-only.
use quickjs_sys as sys;
use std::ffi::CString;
use std::os::raw::c_int;

use crate::js_helpers::{define_getter, define_method, Getter};

unsafe fn alias(ctx: *mut sys::JSContext, global: sys::JSValue, name: &str) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, global, cname.as_ptr(), sys::JS_DupValue(ctx, global));
}

unsafe fn get_scroll_y(ctx: *mut sys::JSContext) -> f64 {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        0.0
    } else {
        (*state).scroll_y
    }
}

unsafe fn set_scroll_y(ctx: *mut sys::JSContext, y: f64) {
    let state = crate::host_state::get(ctx);
    if !state.is_null() {
        (*state).scroll_y = y;
    }
}

/// Reads a JS number value already tagged `INT`/`FLOAT64` (no generic
/// `ToNumber` coercion — same scope cut this crate's other raw-tag number
/// readers document, e.g. `value_bridge`/`history`).
unsafe fn read_number(val: sys::JSValue) -> Option<f64> {
    match val.tag {
        sys::JS_TAG_INT => Some(val.u.int32 as f64),
        sys::JS_TAG_FLOAT64 => Some(val.u.float64),
        _ => None,
    }
}

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str) -> sys::JSValue {
    let name = CString::new(key).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

/// Reads `scroll`/`scrollTo`/`scrollBy`'s real per-spec argument shape:
/// either two numeric coordinates (`x`, `y` — `x` is read and accepted but
/// has no real effect, see this module's own doc on no horizontal scroll)
/// or a single `ScrollToOptions`-shaped object (`{top, left, behavior}` —
/// `behavior` is accepted and ignored, no smooth-scroll animation exists).
/// `None` for either coordinate means "no value supplied" (an omitted
/// `top`, or fewer than 2 numeric args) — callers decide what that means
/// (`scrollBy` treats it as `0`, `scrollTo` as "leave unchanged").
unsafe fn read_scroll_args(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> Option<f64> {
    if argc < 1 {
        return None;
    }
    let first = *argv;
    if first.tag == sys::JS_TAG_OBJECT {
        let top = get_prop(ctx, first, "top");
        let y = read_number(top);
        sys::JS_FreeValue(ctx, top);
        return y;
    }
    if argc < 2 {
        return None;
    }
    read_number(*argv.add(1))
}

unsafe fn fire_scroll_event(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    crate::events::dispatch_simple(ctx, global, "scroll", false, false);
    sys::JS_FreeValue(ctx, global);
}

unsafe extern "C" fn window_scroll_x_get(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(0.0)
}

unsafe extern "C" fn window_scroll_y_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(get_scroll_y(ctx))
}

unsafe extern "C" fn window_inner_width_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    let state = crate::host_state::get(ctx);
    let width = if state.is_null() {
        0.0
    } else {
        (*state).viewport_width
    };
    sys::js_float64(width)
}

unsafe extern "C" fn window_inner_height_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    let state = crate::host_state::get(ctx);
    let height = if state.is_null() {
        0.0
    } else {
        (*state).viewport_height
    };
    sys::js_float64(height)
}

unsafe extern "C" fn window_scroll_to(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if let Some(y) = read_scroll_args(ctx, argc, argv) {
        set_scroll_y(ctx, y);
        fire_scroll_event(ctx);
    }
    sys::js_undefined()
}

unsafe extern "C" fn window_scroll_by(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let dy = read_scroll_args(ctx, argc, argv).unwrap_or(0.0);
    if dy != 0.0 {
        set_scroll_y(ctx, get_scroll_y(ctx) + dy);
        fire_scroll_event(ctx);
    }
    sys::js_undefined()
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    alias(ctx, global, "window");
    alias(ctx, global, "self");
    alias(ctx, global, "top");
    alias(ctx, global, "parent");
    crate::events::define_simple_event_target(ctx, global);

    define_method(
        ctx,
        global,
        "getSelection",
        crate::selection::get_selection,
        0,
    );
    define_getter(ctx, global, "scrollX", window_scroll_x_get as Getter);
    define_getter(ctx, global, "pageXOffset", window_scroll_x_get as Getter);
    define_getter(ctx, global, "scrollY", window_scroll_y_get as Getter);
    define_getter(ctx, global, "pageYOffset", window_scroll_y_get as Getter);
    define_getter(ctx, global, "innerWidth", window_inner_width_get as Getter);
    define_getter(
        ctx,
        global,
        "innerHeight",
        window_inner_height_get as Getter,
    );
    for name in ["scroll", "scrollTo"] {
        define_method(ctx, global, name, window_scroll_to, 2);
    }
    define_method(ctx, global, "scrollBy", window_scroll_by, 2);
    define_method(ctx, global, "open", window_open, 1);

    sys::JS_FreeValue(ctx, global);

    // Real `document` is a `Node` (`Document extends Node`) in a real DOM,
    // so it gets `EventTarget` for free from that hierarchy. This crate's
    // `document` global is a plain object, not a `Node`-class instance (see
    // `crate::document`'s own docs), so it needs the same non-tree-walking
    // `addEventListener`/`dispatchEvent` surface `window` just got, wired
    // here rather than in `crate::document::get_or_create` itself since
    // that function is called repeatedly (idempotent get-or-create) and
    // redefining these methods on every call would be wasteful, not just
    // redundant.
    let document = crate::document::get_or_create(ctx);
    crate::events::define_simple_event_target(ctx, document);
    sys::JS_FreeValue(ctx, document);
}

/// Real `window.open(url)` — `url` is accepted but not fetched/navigated
/// (see `window_registry::open_window`'s own doc for the scope cut).
/// `window_registry::open_window` itself sets the new window's `opener`
/// (a `RemoteWindow` pointing back at the caller), so it's set
/// consistently whether a window was opened via this JS-facing global or
/// directly via `Context::open_window` (the Rust-level entry point).
unsafe extern "C" fn window_open(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let new_id = crate::window_registry::open_window(ctx);
    crate::window_registry::make_remote_window(ctx, new_id)
}
