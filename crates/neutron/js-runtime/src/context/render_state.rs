//! Layout rects, computed styles, CSP, and Permissions-Policy — split out
//! from `context/mod.rs`.

use super::Context;
use crate::Rect;

impl<'rt> Context<'rt> {
    /// Replaces every real layout rect (`getBoundingClientRect`/
    /// `offsetWidth`/etc — see `layout_measurement`) wholesale — a host
    /// (`profile-worker`'s `Page::render`, after it runs `layout-engine`
    /// against the current DOM) calls this once per render pass. No-op on
    /// a plain [`Context::new`], same as [`Context::set_url`].
    pub fn set_layout_rects(&mut self, rects: std::collections::HashMap<dom::NodeId, Rect>) {
        if let Some(state) = self._host_state.as_mut() {
            state.layout_rects = rects;
        }
    }

    /// Replaces every real cascaded computed style (`getComputedStyle` —
    /// see `computed_style`) wholesale — a host (`profile-worker`'s
    /// `Page::render`, after it runs `layout-engine`'s cascade resolver
    /// against the current DOM) calls this once per render pass, same
    /// shape as [`Context::set_layout_rects`]. No-op on a plain
    /// [`Context::new`]/`with_dom` that never calls it.
    pub fn set_computed_styles(
        &mut self,
        styles: std::collections::HashMap<dom::NodeId, std::collections::HashMap<String, String>>,
    ) {
        if let Some(state) = self._host_state.as_mut() {
            state.computed_styles = styles;
        }
    }

    /// Replaces every real per-element scroll extent (`scrollWidth`/
    /// `scrollHeight` — see `layout_measurement`) wholesale — same shape
    /// and call site as [`Context::set_layout_rects`] (`ROADMAP.md` item
    /// 22). No-op on a plain [`Context::new`].
    pub fn set_scroll_extents(
        &mut self,
        extents: std::collections::HashMap<dom::NodeId, (f64, f64)>,
    ) {
        if let Some(state) = self._host_state.as_mut() {
            state.scroll_extents = extents;
        }
    }

    /// Replaces this context's `Content-Security-Policy` policy list with
    /// the single given policy, checked by `fetch`/`fetchSync`/
    /// `XMLHttpRequest` before sending a request (see `csp`). No-op on a
    /// plain [`Context::new`]/`with_dom` that never calls it — nothing
    /// enforces a CSP until a host provides one, same pattern
    /// [`Context::set_url`] already has.
    ///
    /// Wholesale replace (like every other setter here) — a host that
    /// delivers *several* real policies (repeated response headers,
    /// `<meta http-equiv>` tags) and needs them enforced together calls
    /// [`Context::add_csp_policy`] per delivery instead, which appends.
    pub fn set_csp(&mut self, policy: &str) {
        if let Some(state) = self._host_state.as_mut() {
            state.csp = vec![policy.to_string()];
        }
    }

    /// Appends one delivered `Content-Security-Policy` policy to the list
    /// `fetch`/`fetchSync`/`XMLHttpRequest` check — a request must be
    /// allowed by *every* delivered policy (real CSP's multiple-policy
    /// model: policies intersect, they don't merge, so two policies are
    /// kept as two entries rather than joined into one string). A host
    /// (`profile-worker`'s `Page::load`) calls this once per repeated
    /// `Content-Security-Policy` response header and once per
    /// `<meta http-equiv="Content-Security-Policy">` tag, in delivery
    /// order. No-op on a plain [`Context::new`], same as
    /// [`Context::set_csp`].
    pub fn add_csp_policy(&mut self, policy: &str) {
        if let Some(state) = self._host_state.as_mut() {
            state.csp.push(policy.to_string());
        }
    }

    /// Sets the page's `Permissions-Policy` text. The policy is checked at
    /// the native boundary before this context uses clipboard or notification
    /// capabilities, so page JavaScript cannot bypass it by retaining a
    /// reference to either API. No-op on a plain [`Context::new`], which has
    /// no navigated document to associate with a response policy.
    pub fn set_permissions_policy(&mut self, policy: &str) {
        if let Some(state) = self._host_state.as_mut() {
            state.permissions_policy = Some(policy.to_string());
        }
    }

    /// Real `<canvas>` compositing (`crate::canvas_bindings`): every
    /// canvas element that's ever had `getContext('2d')` called on it,
    /// with its *current* drawn pixels (`render::Canvas2D::get_image_data`,
    /// re-read fresh on every call — canvas content can change from a JS
    /// draw call at any point, with no dirty-tracking to know when). A
    /// host (`profile-worker`'s `Page::render`) calls this once per real
    /// paint and feeds the result into `layout_engine::
    /// apply_canvas_snapshots`. Empty on a plain `Context::new`/`with_dom`
    /// that has no `HostState`, or on a page with no canvas that's ever
    /// called `getContext`.
    pub fn canvas_snapshots(&self) -> std::collections::HashMap<dom::NodeId, (u32, u32, Vec<u8>)> {
        let Some(state) = self._host_state.as_ref() else {
            return std::collections::HashMap::new();
        };
        state
            .canvases
            .iter()
            .map(|(node, canvas)| {
                let canvas = canvas.borrow();
                (
                    *node,
                    (canvas.width(), canvas.height(), canvas.get_image_data()),
                )
            })
            .collect()
    }

    /// Whether this context has any canvas element with a real 2D
    /// context — a host checks this to decide whether it must bypass its
    /// own paint/layer caching every frame (see `canvas_snapshots`'s own
    /// doc for why: canvas content can change with no signal this crate
    /// can see cheaply). `false` on a plain `Context::new`/`with_dom`.
    pub fn has_active_canvases(&self) -> bool {
        self._host_state
            .as_ref()
            .map(|state| !state.canvases.is_empty())
            .unwrap_or(false)
    }
}
