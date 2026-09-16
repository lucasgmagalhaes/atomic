//! Console-message draining, pending-navigation, and `beforeunload` —
//! split out from `context/mod.rs`.

use quickjs_sys as sys;

use crate::{console, events, host_state};

use super::Context;

impl<'rt> Context<'rt> {
    /// Drains (and returns) every `console.*` message accumulated since
    /// the last call - the host-facing half of [`crate::console`]: a
    /// devtools-style consumer polls this rather than intercepting each
    /// call. Empty on a plain [`Context::new`] (no host state behind the
    /// buffer).
    pub fn take_console_messages(&self) -> Vec<console::ConsoleMessage> {
        // Through the same opaque-slot pointer the native bindings use
        // (not `_host_state`): `&self` here matches how page scripts hit
        // this buffer mid-eval - interior mutability by convention.
        unsafe {
            let state = host_state::get(self.ptr);
            if state.is_null() {
                Vec::new()
            } else {
                std::mem::take(&mut (*state).console_messages)
            }
        }
    }

    /// Drains and returns this page's pending `<a href>` click-navigation
    /// request, if any — see `host_state::HostState::pending_navigation`'s
    /// own doc. A host calls this after any command that could dispatch a
    /// real click (`CLICK`/`CLICK_AT`/`EVAL`) and, if `Some`, is expected to
    /// actually navigate there (fetch, replace the page, reset scroll/
    /// focus) - this method only reports the request, it doesn't perform
    /// any navigation itself (this crate has no fetch/host-process layer to
    /// do that with). `None` on a plain [`Context::new`].
    pub fn take_pending_navigation(&self) -> Option<String> {
        unsafe {
            let state = host_state::get(self.ptr);
            if state.is_null() {
                None
            } else {
                (*state).pending_navigation.take()
            }
        }
    }

    /// Fires a real, cancelable `beforeunload` on `window` (the global
    /// object) — a host that's about to replace this context's page
    /// (`profile-worker`'s `RELOAD`/`NAVIGATE` handling) calls this first
    /// and only proceeds if it returns `true`. A listener that calls
    /// `event.preventDefault()` makes this return `false`, i.e. the page
    /// asked to stay. Real deviation from spec: a real browser then shows a
    /// confirmation dialog the user can override; this engine has no
    /// dialog/UI-confirmation surface at all yet (see
    /// `JS_ENGINE_CAPABILITY_MATRIX.md`'s "dialogs, popups" gap), so
    /// `preventDefault()` cancels the navigation outright rather than just
    /// requesting confirmation.
    pub fn fire_before_unload(&self) -> bool {
        unsafe {
            let global = sys::JS_GetGlobalObject(self.ptr);
            let proceed = events::dispatch_simple(self.ptr, global, "beforeunload", false, true);
            sys::JS_FreeValue(self.ptr, global);
            proceed
        }
    }
}
