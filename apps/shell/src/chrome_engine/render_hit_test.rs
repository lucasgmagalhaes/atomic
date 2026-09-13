//! Hover/render/hit-testing/click dispatch — split out from
//! `chrome_engine.rs`.

use render::{build_display_list, build_glyph_list, composite_glyphs, GpuRenderer};

use crate::chrome_bridge::{self, ChromeAction};

use super::helpers::js_string_literal;
use super::ChromeEngine;

impl<'rt> ChromeEngine<'rt> {
    /// Real live `:hover`: hit-tests `(x, y)` against the current layout
    /// and updates `dom::Dom`'s hover state to match, skipping the write
    /// entirely (and thus never bumping `style_version`) when the
    /// hit-tested node is already the hovered one — so a stationary mouse
    /// doesn't force a box-tree rebuild every frame. `&mut self` (unlike
    /// every other method here, which mutates the DOM indirectly through
    /// a JS `eval` and so only needs `&self`) because this is the one
    /// caller that mutates `dom::Dom` directly via `Context::dom_mut`,
    /// which requires `&mut Context`. A caller (e.g. `chrome_toolbar_ui.rs`)
    /// should call this once per frame, before [`render`](Self::render),
    /// using its own per-frame pointer position.
    pub(crate) fn update_hover(&mut self, width: u32, x: f64, y: f64) {
        let tree = self.layout(width);
        let hit = layout_engine::hit_test(&tree, x, y);
        let dom = self
            .ctx
            .dom()
            .expect("ChromeEngine always builds its context over a dom");
        let currently_hovered = dom.hovered_element();
        match hit {
            Some(node) if currently_hovered != Some(node) => {
                self.ctx
                    .dom_mut()
                    .expect("ChromeEngine always builds its context over a dom")
                    .set_hovered(node);
            }
            None if currently_hovered.is_some() => {
                self.ctx
                    .dom_mut()
                    .expect("ChromeEngine always builds its context over a dom")
                    .clear_hover();
            }
            _ => {}
        }
    }

    /// Renders this chrome surface to an RGBA8 buffer, same two-pass
    /// pipeline (`GpuRenderer` rects, then CPU glyph compositing) a
    /// `profile-worker` page uses — reused verbatim via the `render` crate,
    /// not a second rendering path.
    pub(crate) fn render(&self, renderer: &GpuRenderer, width: u32, height: u32) -> Vec<u8> {
        let tree = self.layout(width);
        let rects = build_display_list(&tree);
        let glyphs = build_glyph_list(&tree);
        let mut pixels = renderer.render_to_rgba(&rects, width, height, [0.12, 0.13, 0.17, 1.0]);
        composite_glyphs(&mut pixels, width, height, &glyphs);
        pixels
    }

    /// Hit-tests `(x, y)` against the same tree `render` used and returns
    /// the nearest `id`-addressable ancestor of whatever's under the point
    /// — same limitation and reasoning
    /// `profile_worker::input_commands::dispatch_click_at` documents (only
    /// elements with an `id`, direct or via bubbling, are reachable this
    /// way; every clickable element in a chrome bundle carries one for
    /// exactly this). Split out from dispatch so a caller can inspect
    /// (e.g. read an `<input>`'s live value via [`input_value`]) *before*
    /// the click's own handler runs and possibly mutates the DOM further.
    pub(crate) fn resolve_click_target(&self, width: u32, x: f64, y: f64) -> Option<String> {
        let tree = self.layout(width);
        let node = layout_engine::hit_test(&tree, x, y)?;
        let dom = self
            .ctx
            .dom()
            .expect("ChromeEngine always builds its context over a dom");
        let mut current = Some(node);
        while let Some(id) = current {
            if let Some(value) = dom.attribute(id, "id") {
                return Some(value.to_string());
            }
            current = dom.get(id).and_then(|n| n.parent);
        }
        None
    }

    /// Dispatches a real `"click"` DOM event at the element with `id`, same
    /// convention `profile_worker::input_commands::dispatch_click` uses
    /// (`el.dispatchEvent("click")` via the existing JS binding).
    pub(crate) fn dispatch_click(&self, id: &str) {
        let script = format!(
            "(function(){{ var el = document.getElementById({id_js}); if (el) el.dispatchEvent(\"click\"); }})();",
            id_js = js_string_literal(id),
        );
        let _ = self.ctx.eval(&script, "<chrome click>");
    }

    /// Convenience: resolves and dispatches in one call, for chrome
    /// surfaces (like the toolbar) with no input fields to read first.
    pub(crate) fn handle_click(&self, width: u32, x: f64, y: f64) {
        if let Some(id) = self.resolve_click_target(width, x, y) {
            self.dispatch_click(&id);
        }
    }

    /// Drains and returns every `atomic.*` action this chrome surface's JS
    /// queued since the last call — `AtomicApp::update` dispatches each one
    /// into its own real methods.
    pub(crate) fn drain_actions(&self) -> Vec<ChromeAction> {
        chrome_bridge::drain_actions(self.ctx.as_raw())
    }
}
