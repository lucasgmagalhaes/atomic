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
//! spec-read-only. `window.visualViewport` reuses the same two backing
//! fields for its own `width`/`height` (this engine models no pinch-zoom,
//! so the visual and layout viewports are always identical in size) —
//! see [`make_visual_viewport`]'s own doc for its scope cut (a plain
//! object, not a real `EventTarget`).
//!
//! Also real `alert`/`confirm`/`prompt` (`spec/matrix/browser-apis.md`'s
//! Device/UI row): each is a real, callable global that coerces its
//! message argument to a string and records it via
//! `console::push_message` (same `HostState.console_messages` sink
//! `console.log` already feeds, tagged `[alert]`/`[confirm]`/`[prompt]`
//! so a host draining that stream can tell a dialog call apart from
//! ordinary logging) — deliberately scoped to a headless engine with no
//! user to click a button: real spec's own dialogs block script
//! execution until a person responds, which this engine has no UI or
//! event loop to do, so each call returns immediately with real spec's
//! own "no interactivity" default (`alert`: `undefined`; `confirm`:
//! `false`, matching a real dialog a user never confirmed; `prompt`:
//! `null`, matching a real dialog a user cancelled) rather than
//! blocking or throwing. No host-settable override exists yet — a test
//! that needs to simulate a user answering "OK" can't today; that's a
//! real, narrower-than-spec limitation, not a silent gap (a page
//! branching on `confirm()`'s result always takes the same branch here).
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

/// `window.visualViewport.scale` — always `1.0`. This engine has no
/// pinch-zoom/mobile-viewport-meta-tag model, so there's no real
/// browser-UI-vs-layout-viewport distinction to report; a plain `1.0`
/// (never-zoomed) is the honest constant, not a placeholder standing in
/// for something unimplemented.
unsafe extern "C" fn visual_viewport_scale_get(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(1.0)
}

/// `window.visualViewport.offsetLeft`/`.offsetTop`/`.pageLeft`/
/// `.pageTop` — always `0.0`, same "no pinch-zoom/browser-chrome-overlap
/// model" reasoning as [`visual_viewport_scale_get`]: with no zoom, the
/// visual and layout viewports are always identical, so every offset
/// between them is genuinely zero, not an unimplemented placeholder.
unsafe extern "C" fn visual_viewport_zero_get(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(0.0)
}

/// `window.visualViewport` — a plain object (not a real `EventTarget`;
/// no `addEventListener('resize'/'scroll', ...)` — a page wanting to
/// react to viewport size changes uses `window`'s own real `"resize"`
/// event instead, see `crate::context::viewport::Context::fire_resize`).
/// `width`/`height` reuse [`window_inner_width_get`]/
/// [`window_inner_height_get`] directly (same `this_val`-ignoring shape,
/// reading straight from `HostState`) since with no pinch-zoom modeled
/// the visual viewport is always identical in size to the layout
/// viewport `innerWidth`/`innerHeight` already report.
unsafe fn make_visual_viewport(ctx: *mut sys::JSContext) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    define_getter(ctx, obj, "width", window_inner_width_get as Getter);
    define_getter(ctx, obj, "height", window_inner_height_get as Getter);
    define_getter(ctx, obj, "scale", visual_viewport_scale_get as Getter);
    for name in ["offsetLeft", "offsetTop", "pageLeft", "pageTop"] {
        define_getter(ctx, obj, name, visual_viewport_zero_get as Getter);
    }
    obj
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

unsafe fn read_message_arg(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> String {
    if argc < 1 {
        return "undefined".to_string();
    }
    let mut len: usize = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, *argv, false);
    if ptr.is_null() {
        return String::new();
    }
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
    let s = String::from_utf8_lossy(bytes).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    s
}

unsafe extern "C" fn window_alert(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let message = read_message_arg(ctx, argc, argv);
    crate::console::push_message(
        ctx,
        crate::console::ConsoleMessage {
            level: crate::console::ConsoleLevel::Log,
            text: format!("[alert] {message}"),
        },
    );
    sys::js_undefined()
}

unsafe extern "C" fn window_confirm(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let message = read_message_arg(ctx, argc, argv);
    crate::console::push_message(
        ctx,
        crate::console::ConsoleMessage {
            level: crate::console::ConsoleLevel::Log,
            text: format!("[confirm] {message}"),
        },
    );
    sys::js_bool(false)
}

unsafe extern "C" fn window_prompt(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let message = read_message_arg(ctx, argc, argv);
    crate::console::push_message(
        ctx,
        crate::console::ConsoleMessage {
            level: crate::console::ConsoleLevel::Log,
            text: format!("[prompt] {message}"),
        },
    );
    sys::js_null()
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
    {
        let visual_viewport = make_visual_viewport(ctx);
        let name_c = CString::new("visualViewport").unwrap();
        sys::JS_SetPropertyStr(ctx, global, name_c.as_ptr(), visual_viewport);
    }
    for name in ["scroll", "scrollTo"] {
        define_method(ctx, global, name, window_scroll_to, 2);
    }
    define_method(ctx, global, "scrollBy", window_scroll_by, 2);
    define_method(ctx, global, "open", window_open, 1);
    define_method(ctx, global, "alert", window_alert, 1);
    define_method(ctx, global, "confirm", window_confirm, 1);
    define_method(ctx, global, "prompt", window_prompt, 2);

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
