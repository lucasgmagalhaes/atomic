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

/// Registers `document.visibilityState` and `document.hidden` on `ctx`,
/// reusing the shared `document` global (see `crate::document`) so this
/// doesn't clobber whatever `dom_bindings::register` already put there
/// (or vice versa, depending on registration order).
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let document = crate::document::get_or_create(ctx);

    let visibility_name = CString::new("visibilityState").unwrap();
    let visibility_value =
        sys::JS_NewStringLen(ctx, b"visible".as_ptr() as *const std::os::raw::c_char, 7);
    sys::JS_SetPropertyStr(ctx, document, visibility_name.as_ptr(), visibility_value);

    let hidden_name = CString::new("hidden").unwrap();
    sys::JS_SetPropertyStr(ctx, document, hidden_name.as_ptr(), sys::js_bool(false));

    sys::JS_FreeValue(ctx, document);
}
