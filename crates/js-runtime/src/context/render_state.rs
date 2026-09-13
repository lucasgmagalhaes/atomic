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
}
