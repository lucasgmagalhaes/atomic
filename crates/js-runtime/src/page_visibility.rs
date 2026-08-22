//! `document.visibilityState` / `document.hidden`, hardcoded to "visible"
//! for now. The real Page Visibility API reflects whether the tab/pane is
//! actually foregrounded, which needs the shell to tell a profile's
//! context when its pane gets backgrounded (tiling/focus tracking that
//! doesn't exist yet — that's `apps/shell` + `profile`/`ipc`, phase 4).
//! Static properties, not getters: there is nothing live to read yet, and
//! a getter that always returns the same constant would just be a more
//! roundabout way to say the same thing.
use std::ffi::CString;

use quickjs_sys as sys;

/// Registers `document.visibilityState` and `document.hidden` on `ctx`.
/// Creates its own `document` global object — `dom_bindings::register`
/// only adds bare `__dom_*` globals, it doesn't create one, so there's
/// nothing to share yet. Once DOM bindings grow a real `document` object
/// (needed anyway for the eventual `getElementById`), these two properties
/// move onto it instead of each owning a separate `document`.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    let document = sys::JS_NewObject(ctx);

    let visibility_name = CString::new("visibilityState").unwrap();
    let visibility_value = sys::JS_NewStringLen(
        ctx,
        b"visible".as_ptr() as *const std::os::raw::c_char,
        7,
    );
    sys::JS_SetPropertyStr(ctx, document, visibility_name.as_ptr(), visibility_value);

    let hidden_name = CString::new("hidden").unwrap();
    sys::JS_SetPropertyStr(ctx, document, hidden_name.as_ptr(), sys::js_bool(false));

    let document_name = CString::new("document").unwrap();
    sys::JS_SetPropertyStr(ctx, global, document_name.as_ptr(), document);

    sys::JS_FreeValue(ctx, global);
}
