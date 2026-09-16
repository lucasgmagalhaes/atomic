//! Scroll offset, viewport size, and `"resize"` dispatch — split out from
//! `context/mod.rs`.

use quickjs_sys as sys;

use super::Context;

impl<'rt> Context<'rt> {
    /// Sets the real document/viewport vertical scroll offset backing
    /// `window.scrollY`/`pageYOffset` (see `crate::window` and
    /// `host_state::HostState::scroll_y`'s own doc for why this field is
    /// two-way, unlike every other setter here). A host calls this after
    /// its own input handling moves the viewport (a wheel/scrollbar event);
    /// [`Context::scroll_y`] is the other half, letting the host read back
    /// a value a page's own script changed via `scrollTo`/`scrollBy` before
    /// the host's next paint. No-op on a plain [`Context::new`], same as
    /// [`Context::set_url`].
    pub fn set_scroll_y(&mut self, y: f64) {
        if let Some(state) = self._host_state.as_mut() {
            state.scroll_y = y;
        }
    }

    /// Reads the real document/viewport vertical scroll offset — see
    /// [`Context::set_scroll_y`]'s doc. `0.0` on a plain [`Context::new`]
    /// (no host state to read from).
    pub fn scroll_y(&self) -> f64 {
        self._host_state.as_ref().map(|s| s.scroll_y).unwrap_or(0.0)
    }

    /// Sets the real viewport size backing `window.innerWidth`/
    /// `innerHeight` (see `crate::window` and
    /// `host_state::HostState::viewport_width`'s own doc). One-way, unlike
    /// [`Context::set_scroll_y`]: called by the host whenever it lays a
    /// page out against a real size (initial spawn, or a live resize),
    /// with no JS-facing setter since the real properties are
    /// spec-read-only. No-op on a plain [`Context::new`].
    pub fn set_viewport_size(&mut self, width: f64, height: f64) {
        if let Some(state) = self._host_state.as_mut() {
            state.viewport_width = width;
            state.viewport_height = height;
        }
    }

    /// Reads the real viewport width — see [`Context::set_viewport_size`]'s
    /// doc. `0.0` on a plain [`Context::new`] (no host state to read from).
    pub fn viewport_width(&self) -> f64 {
        self._host_state
            .as_ref()
            .map(|s| s.viewport_width)
            .unwrap_or(0.0)
    }

    /// Reads the real viewport height — see [`Context::set_viewport_size`]'s
    /// doc.
    pub fn viewport_height(&self) -> f64 {
        self._host_state
            .as_ref()
            .map(|s| s.viewport_height)
            .unwrap_or(0.0)
    }

    /// Dispatches a real, synchronous `"resize"` event on `window` — same
    /// convention `window`'s own scroll dispatch uses (see
    /// `crate::window::fire_scroll_event`), just callable by the host
    /// directly rather than only from a JS-facing setter, since a resize
    /// is host-initiated (a live `RESIZE` command), not driven by script.
    /// A host calls this right after [`Context::set_viewport_size`] so a
    /// page's own `"resize"` listener sees the new `innerWidth`/
    /// `innerHeight` already in place.
    pub fn fire_resize(&self) {
        unsafe {
            let global = sys::JS_GetGlobalObject(self.ptr);
            crate::events::dispatch_simple(self.ptr, global, "resize", false, false);
            sys::JS_FreeValue(self.ptr, global);
        }
    }
}
