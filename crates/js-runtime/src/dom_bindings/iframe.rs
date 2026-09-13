//! `<iframe>` (`ROADMAP.md` item 35): real `.contentWindow`, backed by a
//! genuinely independent child browsing context opened lazily on first
//! access via `window_registry::open_window` — the same primitive
//! `window.open()` (items 36/37) already uses, just triggered by reading
//! this property instead of calling a global function.
//!
//! Scope cut, deliberate (see `window_registry.rs`'s own module doc for
//! the full reasoning): no `.contentDocument` — this engine has no
//! mechanism to share a live object from a *different* `JSContext`
//! directly into the parent's own realm, only the structured-clone-
//! shaped message passing `.contentWindow.postMessage()` already gives.
//! No visual embedding either — `layout-engine` has no concept of
//! nesting a second document inside a box, so an `<iframe>` lays out and
//! paints as an ordinary (empty) replaced-nothing element, same as any
//! other tag this engine doesn't give special rendering treatment.
//! `src` is accepted as a plain reflected attribute but never fetched —
//! matching `window.open(url)`'s own "starts blank" scope cut.

use quickjs_sys as sys;

use super::node_registry::node_id;

unsafe extern "C" fn iframe_content_window_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_null();
    };
    let window_id = crate::window_registry::window_for_iframe(ctx, id);
    crate::window_registry::make_remote_window(ctx, window_id)
}

/// Defines `.contentWindow` on `HTMLIFrameElement.prototype`.
pub(super) unsafe fn define_iframe_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    crate::js_helpers::define_getter(ctx, proto, "contentWindow", iframe_content_window_get);
}
