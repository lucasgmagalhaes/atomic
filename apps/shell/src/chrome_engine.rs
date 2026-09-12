//! Spike: one piece of shell chrome (the toolbar) rendered by Nimble's own
//! engine instead of egui — proves the Track B architecture (in-process
//! engine instance, native JS bridge, GPU-buffer compositing) end to end
//! before the rest of the shell's chrome migrates the same way. Deliberately
//! self-contained rather than reusing `profile::bin::profile_worker::Page`
//! (which is private to that binary crate) — this mirrors its load/layout/
//! render/hit-test shape using the same public `html`/`css`/`layout-engine`/
//! `render`/`js-runtime` crates, minus what a static local chrome bundle
//! never needs (navigation, images, CSP, cookies/storage).

use std::cell::RefCell;

use css::{parse_stylesheet, Stylesheet};
use dom::NodeId;
use js_runtime::{Context, Runtime};
use layout_engine::{build_box_tree_with_viewport, layout_block, LayoutBox};
use render::{build_display_list, build_glyph_list, composite_glyphs, GpuRenderer};

use crate::chrome_bridge::{self, ChromeAction};

/// One loaded chrome surface (e.g. the toolbar): its DOM root, stylesheet,
/// and the JS context driving it, plus a cached box tree so `render` and
/// `hit_test` never disagree — same reasoning `Page::layout`'s cache
/// documents for a real page.
pub(crate) struct ChromeEngine<'rt> {
    ctx: Context<'rt>,
    root: NodeId,
    sheet: Stylesheet,
    layout_cache: RefCell<Option<LayoutCache>>,
    /// The `id` of whichever `<input>`/`<textarea>` a click most recently
    /// landed on — set by [`click_at_with_focus`](Self::click_at_with_focus),
    /// consumed by [`type_key`](Self::type_key). `None` after a click on
    /// anything non-input-like, same "clicking away blurs" behavior
    /// `profile_worker::input_commands::dispatch_click_at` documents (minus
    /// this spike's simplification of not also dispatching real `blur`/
    /// `change` events — nothing in the bundles built on this yet depends
    /// on either).
    focused_input: RefCell<Option<String>>,
}

struct LayoutCache {
    width: u32,
    layout_ver: u64,
    /// `dom::Dom::style_version()` at the time this tree was built — a
    /// hover/focus change bumps this without bumping `layout_ver` (see
    /// that field's own doc), so keying the cache on it too is what makes
    /// `update_hover`'s DOM writes actually visible in the next `render`
    /// instead of silently serving a stale cached tree forever.
    style_ver: u64,
    tree: LayoutBox,
}

impl<'rt> ChromeEngine<'rt> {
    /// The toolbar spike's bundle — `apps/shell/chrome/toolbar.{html,css,js}`.
    pub(crate) fn new_toolbar(runtime: &'rt Runtime) -> Self {
        Self::new(
            runtime,
            include_str!("../chrome/toolbar.html"),
            include_str!("../chrome/toolbar.css"),
            include_str!("../chrome/toolbar.js"),
        )
    }

    /// The Settings window's bundle —
    /// `apps/shell/chrome/settings.{html,css,js}`.
    pub(crate) fn new_settings(runtime: &'rt Runtime) -> Self {
        Self::new(
            runtime,
            include_str!("../chrome/settings.html"),
            include_str!("../chrome/settings.css"),
            include_str!("../chrome/settings.js"),
        )
    }

    /// The downloads/history panel's bundle —
    /// `apps/shell/chrome/downloads_history.{html,css,js}`.
    pub(crate) fn new_downloads_history(runtime: &'rt Runtime) -> Self {
        Self::new(
            runtime,
            include_str!("../chrome/downloads_history.html"),
            include_str!("../chrome/downloads_history.css"),
            include_str!("../chrome/downloads_history.js"),
        )
    }

    /// Loads a fixed local HTML/CSS/JS bundle (no navigation, no network —
    /// chrome is not a tab). Registers `globalThis.nimble.*`
    /// (`chrome_bridge::register`) before running `js`, so the bundle's own
    /// script can wire button handlers immediately.
    fn new(runtime: &'rt Runtime, html: &str, css_text: &str, js: &str) -> Self {
        let (dom, root) = html::parse_to_html_element(html);
        let sheet = parse_stylesheet(css_text);
        let ctx = Context::with_dom(runtime, dom);
        unsafe {
            chrome_bridge::register(ctx.as_raw());
        }
        ctx.eval(js, "chrome-toolbar.js")
            .expect("chrome bundle script is static and must not fail to eval");
        ctx.dispatch_lifecycle_events();
        ChromeEngine {
            ctx,
            root,
            sheet,
            layout_cache: RefCell::new(None),
            focused_input: RefCell::new(None),
        }
    }

    /// The Add Profile modal's bundle —
    /// `apps/shell/chrome/add_profile.{html,css,js}`.
    pub(crate) fn new_add_profile(runtime: &'rt Runtime) -> Self {
        Self::new(
            runtime,
            include_str!("../chrome/add_profile.html"),
            include_str!("../chrome/add_profile.css"),
            include_str!("../chrome/add_profile.js"),
        )
    }

    /// Builds/lays out this chrome surface's box tree against `width`,
    /// reusing the cached tree only when both `width` and the DOM's own
    /// layout version match the cached entry — same cache key `Page::layout`
    /// uses (minus `adopted_version`; chrome has no `adoptedStyleSheets`
    /// today). Getting this check right matters more here than for a real
    /// page: `sync_toolbar_state`/`sync_downloads_history`'s `innerHTML`/
    /// `className` mutations are this chrome surface's *only* way to
    /// reflect changed `NimbleApp` state, so a cache that never
    /// re-validates against `layout_ver` would render the very first frame
    /// forever regardless of what the sync calls do.
    fn layout(&self, width: u32) -> LayoutBox {
        let dom = self
            .ctx
            .dom()
            .expect("ChromeEngine always builds its context over a dom");
        let layout_ver = dom.layout_version();
        let style_ver = dom.style_version();
        if let Some(cached) = self.layout_cache.borrow().as_ref() {
            if cached.width == width
                && cached.layout_ver == layout_ver
                && cached.style_ver == style_ver
            {
                return cached.tree.clone();
            }
        }
        // `layout_engine::DEFAULT_VIEWPORT_HEIGHT`, not this surface's own
        // fixed `CHROME_HEIGHT` constant: `ChromeEngine::layout` only
        // takes `width` (see this method's own cache key) - no chrome
        // bundle uses a height-based `@media` rule today, so plumbing the
        // real per-surface height through here has no observable effect
        // yet and isn't worth the ripple until one does.
        let mut tree = build_box_tree_with_viewport(
            dom,
            self.root,
            &self.sheet,
            width as f64,
            layout_engine::DEFAULT_VIEWPORT_HEIGHT,
        )
        .expect("chrome bundle's HTML always produces a box tree");
        layout_block(&mut tree, width as f64, 0.0, 0.0);
        *self.layout_cache.borrow_mut() = Some(LayoutCache {
            width,
            layout_ver,
            style_ver,
            tree: tree.clone(),
        });
        tree
    }

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

    /// Same as [`handle_click`](Self::handle_click), plus real focus
    /// tracking: if the resolved target is an `<input>`/`<textarea>`, it
    /// becomes `focused_input` (and gets a real `.focus()` call) so
    /// [`type_key`](Self::type_key) knows where to route subsequent
    /// keystrokes; clicking anything else clears it. Returns the resolved
    /// element id, same as `resolve_click_target`, so a caller that also
    /// needs to special-case a specific button (e.g. a submit) doesn't have
    /// to hit-test twice.
    pub(crate) fn click_at_with_focus(&self, width: u32, x: f64, y: f64) -> Option<String> {
        let id = self.resolve_click_target(width, x, y)?;
        let is_input = {
            let dom = self
                .ctx
                .dom()
                .expect("ChromeEngine always builds its context over a dom");
            dom.find_by_id(&id).is_some_and(|node| {
                matches!(
                    &dom.get(node).map(|n| &n.data),
                    Some(dom::NodeData::Element { tag, .. }) if tag == "input" || tag == "textarea"
                )
            })
        };
        *self.focused_input.borrow_mut() = if is_input { Some(id.clone()) } else { None };
        if is_input {
            self.focus(&id);
        }
        self.dispatch_click(&id);
        Some(id)
    }

    /// Real focus, via the JS `.focus()` binding — same
    /// `profile_worker::input_commands::focus_element` convention (routes
    /// through JS so a real `"focus"` event dispatches too).
    fn focus(&self, id: &str) {
        let script = format!(
            "(function(){{ var el = document.getElementById({id_js}); if (el) el.focus(); }})();",
            id_js = js_string_literal(id),
        );
        let _ = self.ctx.eval(&script, "<chrome focus>");
    }

    /// Types `key` into whichever field a prior [`click_at_with_focus`]
    /// call focused — no-op if nothing is focused. Same semantics as
    /// `profile_worker::input_commands::type_key`: `"Backspace"` removes
    /// the last character, anything else is appended verbatim; a real
    /// `"keydown"` dispatches first via the existing `dispatchEvent`
    /// binding before `.value` is updated.
    pub(crate) fn type_key(&self, key: &str) {
        let Some(id) = self.focused_input.borrow().clone() else {
            return;
        };
        let current = {
            let dom = self
                .ctx
                .dom()
                .expect("ChromeEngine always builds its context over a dom");
            let Some(node) = dom.find_by_id(&id) else {
                return;
            };
            dom.value(node)
        };
        let updated = if key == "Backspace" {
            let mut chars: Vec<char> = current.chars().collect();
            chars.pop();
            chars.into_iter().collect()
        } else {
            format!("{current}{key}")
        };
        let script = format!(
            "(function(){{ var el = document.getElementById({id_js}); if (el) {{ el.dispatchEvent(\"keydown\"); el.value = {value_js}; }} }})();",
            id_js = js_string_literal(&id),
            value_js = js_string_literal(&updated),
        );
        let _ = self.ctx.eval(&script, "<chrome key>");
    }

    /// Sets several `<input>`/`<textarea>` values at once (`id`, `value`
    /// pairs) via real `.value =` writes — used to prefill/reset a form
    /// when it's freshly (re)opened, not called every frame (that would
    /// fight with what the user is actively typing).
    pub(crate) fn set_input_values(&self, values: &[(&str, &str)]) {
        let mut script = String::new();
        for (id, value) in values {
            script.push_str(&format!(
                "(function(){{ var el = document.getElementById({id_js}); if (el) el.value = {value_js}; }})();",
                id_js = js_string_literal(id),
                value_js = js_string_literal(value),
            ));
        }
        let _ = self.ctx.eval(&script, "<chrome prefill>");
    }

    /// Reads the live `.value` of the `<input>`/`<textarea>` with `id` —
    /// `dom::Dom::value`, a real field independent of the `value`
    /// *attribute* (see that method's own doc), so this reflects whatever
    /// the user actually typed, not just an initial static value.
    pub(crate) fn input_value(&self, id: &str) -> Option<String> {
        let dom = self
            .ctx
            .dom()
            .expect("ChromeEngine always builds its context over a dom");
        let node = dom.find_by_id(id)?;
        Some(dom.value(node))
    }

    /// Drains and returns every `nimble.*` action this chrome surface's JS
    /// queued since the last call — `NimbleApp::update` dispatches each one
    /// into its own real methods.
    pub(crate) fn drain_actions(&self) -> Vec<ChromeAction> {
        chrome_bridge::drain_actions(self.ctx.as_raw())
    }

    /// Rewrites the toolbar bundle's `#workspaces`/`#locale` dynamic
    /// regions to reflect current `NimbleApp` state — same "script mutates
    /// the DOM, next render picks it up" pattern a real page's own
    /// `setInterval` callback already relies on (see `Page::DEMO_SCRIPT`'s
    /// doc). `#toolbar`'s click listener is attached once at load time via
    /// event delegation (`toolbar.js`), so replacing `#workspaces`' inner
    /// buttons here doesn't lose it — only per-button `addEventListener`
    /// handlers would need re-attaching, which delegation avoids entirely.
    /// `workspaces` is `(name, is_active)` pairs in display order;
    /// `locale_is_en` picks which of the two locale buttons gets the
    /// `active` class.
    pub(crate) fn sync_toolbar_state(&self, workspaces: &[(String, bool)], locale_is_en: bool) {
        let mut buttons = String::new();
        for (i, (name, active)) in workspaces.iter().enumerate() {
            let class = if *active { "ws-active" } else { "" };
            buttons.push_str(&format!(
                "<button id=\"workspace-{i}\" class=\"{class}\">{}</button>",
                html_escape(name)
            ));
        }
        buttons.push_str("<button id=\"new-workspace\">+ New</button>");

        self.set_inner_html("workspaces", &buttons);
        self.set_class_name("locale-en", if locale_is_en { "ws-active" } else { "" });
        self.set_class_name("locale-pt", if !locale_is_en { "ws-active" } else { "" });
    }

    /// Rewrites the downloads/history bundle's two list containers to
    /// reflect `self.panes[selected].downloads`/`.history` — same
    /// generalized "Rust builds HTML, JS assigns it via innerHTML" pattern
    /// [`sync_toolbar_state`] established. `downloads`/`history` are
    /// already-formatted display lines (the caller decides formatting,
    /// same as `side_panel_ui.rs`'s egui version does per-record).
    pub(crate) fn sync_downloads_history(&self, downloads: &[String], history: &[String]) {
        let render_list = |items: &[String], empty_text: &str| -> String {
            if items.is_empty() {
                format!("<span class=\"empty\">{empty_text}</span>")
            } else {
                items
                    .iter()
                    .map(|line| format!("<div class=\"row\">{}</div>", html_escape(line)))
                    .collect()
            }
        };
        self.set_inner_html(
            "downloads-list",
            &render_list(downloads, "No downloads yet."),
        );
        self.set_inner_html("history-list", &render_list(history, "No history yet."));
    }

    /// Rewrites `#error`'s text — used by the add-profile modal to reflect
    /// `NimbleApp::add_profile_error` every frame (safe to do every frame,
    /// unlike [`set_input_values`](Self::set_input_values): this is
    /// read-only display, never something the user is typing into).
    pub(crate) fn set_error_text(&self, text: &str) {
        self.set_inner_html("error", &html_escape(text));
    }

    /// Rewrites every dynamic region of the Settings bundle in one call —
    /// same "Rust builds HTML/class strings, JS assigns them" pattern
    /// [`sync_toolbar_state`](Self::sync_toolbar_state) established,
    /// generalized to a surface with several independent dynamic regions
    /// instead of just one list. `adapters` are display labels in
    /// `render::list_adapters()`'s own order; `selected_adapter`/
    /// `credential_keys`/`bookmarks` mirror `NimbleApp`'s own state
    /// directly so the caller doesn't need to pre-format anything beyond
    /// what it already has.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sync_settings_state(
        &self,
        max_panes: u32,
        fps_cap: Option<u32>,
        use_keychain: bool,
        adapters: &[String],
        selected_adapter: Option<usize>,
        credential_keys: &[String],
        performance_error: Option<&str>,
        vault_error: Option<&str>,
        import_result: Option<&str>,
        bookmarks: &[String],
    ) {
        self.set_inner_html("panes-value", &max_panes.to_string());
        self.set_class_name("fps-cap-off", if fps_cap.is_none() { "active" } else { "" });
        self.set_class_name("fps-cap-on", if fps_cap.is_some() { "active" } else { "" });
        self.set_inner_html(
            "fps-value",
            &fps_cap
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
        );
        self.set_class_name("keychain-off", if !use_keychain { "active" } else { "" });
        self.set_class_name("keychain-on", if use_keychain { "active" } else { "" });

        let mut adapter_html = String::from(
            "<button id=\"adapter-default\" class=\"active-if-none\">Default</button>",
        );
        for (i, name) in adapters.iter().enumerate() {
            adapter_html.push_str(&format!(
                "<button id=\"adapter-{i}\">{}</button>",
                html_escape(name)
            ));
        }
        self.set_inner_html("adapter-list", &adapter_html);
        // The default button's active class is set separately from the
        // indexed ones (its id is fixed, not part of the loop above) so
        // `selected_adapter == None` highlights it without a special case
        // inside the loop.
        self.set_class_name(
            "adapter-default",
            if selected_adapter.is_none() {
                "active-if-none active"
            } else {
                "active-if-none"
            },
        );
        for i in 0..adapters.len() {
            self.set_class_name(
                &format!("adapter-{i}"),
                if selected_adapter == Some(i) {
                    "active"
                } else {
                    ""
                },
            );
        }

        let credential_html: String = if credential_keys.is_empty() {
            "<span class=\"empty\">No stored credentials.</span>".to_string()
        } else {
            credential_keys
                .iter()
                .enumerate()
                .map(|(i, key)| {
                    format!(
                        "<div class=\"row\"><span>{}</span><button id=\"cred-remove-{i}\">Remove</button></div>",
                        html_escape(key)
                    )
                })
                .collect()
        };
        self.set_inner_html("credential-list", &credential_html);

        self.set_inner_html("error", performance_error.unwrap_or(""));
        self.set_inner_html("vault-error", vault_error.unwrap_or(""));
        self.set_inner_html("import-result", &html_escape(import_result.unwrap_or("")));

        let bookmark_html: String = bookmarks
            .iter()
            .map(|line| format!("<div class=\"row\">{}</div>", html_escape(line)))
            .collect();
        self.set_inner_html("bookmark-list", &bookmark_html);
    }

    /// Generic innerHTML rewrite for `#id` — the shared primitive both
    /// `sync_toolbar_state` and `sync_downloads_history` build on.
    fn set_inner_html(&self, id: &str, html: &str) {
        let script = format!(
            "document.getElementById({id_js}).innerHTML = {html_js};",
            id_js = js_string_literal(id),
            html_js = js_string_literal(html),
        );
        let _ = self.ctx.eval(&script, "<chrome sync>");
    }

    /// Generic `className` rewrite for `#id`.
    fn set_class_name(&self, id: &str, class_name: &str) {
        let script = format!(
            "document.getElementById({id_js}).className = {class_js};",
            id_js = js_string_literal(id),
            class_js = js_string_literal(class_name),
        );
        let _ = self.ctx.eval(&script, "<chrome sync>");
    }
}

impl<'rt> Drop for ChromeEngine<'rt> {
    /// Evicts this surface's entry from `chrome_bridge`'s thread-local
    /// registry (keyed by raw `*mut JSContext` as `usize`) before the
    /// `Context` itself drops — same reasoning `class_registry.rs` documents
    /// for why `Runtime::drop` must evict its own registry entries first:
    /// without this, a later `Context` allocated at the same freed address
    /// could inherit a leftover queued action from this one. Currently
    /// unreachable in practice (`NimbleApp` leaks every chrome `Runtime` via
    /// `Box::leak`, so no `ChromeEngine` is ever actually dropped), but the
    /// bridge shouldn't rely on that staying true.
    fn drop(&mut self) {
        let _ = chrome_bridge::drain_actions(self.ctx.as_raw());
    }
}

/// Minimal HTML-text escaping for interpolating a Rust string (a workspace
/// name) into the innerHTML this module builds — not a general sanitizer,
/// just enough that a name containing `<`/`&` can't break the markup
/// structure `sync_toolbar_state` assembles.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A valid JS double-quoted string literal for `s` — same escaping
/// `profile_worker::input_commands::js_string_literal` uses for embedding
/// an arbitrary Rust string into a small `eval`ed JS snippet.
fn js_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
