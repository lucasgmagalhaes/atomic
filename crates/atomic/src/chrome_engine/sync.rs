//! `sync_*`/`set_*` DOM-mutation methods — split out from
//! `chrome_engine.rs`.

use super::helpers::{html_escape, js_string_array, js_string_literal, js_workspace_array};
use super::ChromeEngine;

impl<'rt> ChromeEngine<'rt> {
    /// Rewrites the toolbar bundle's `#workspaces`/`#locale` dynamic
    /// regions to reflect current `AtomicApp` state — same "script mutates
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
        self.render_call("renderWorkspaces", &js_workspace_array(workspaces));
        self.set_class_name("locale-en", if locale_is_en { "btn active" } else { "btn" });
        self.set_class_name(
            "locale-pt",
            if !locale_is_en { "btn active" } else { "btn" },
        );
    }

    /// Rewrites the downloads/history bundle's two list containers to
    /// reflect `self.panes[selected].downloads`/`.history` — same
    /// generalized "Rust builds HTML, JS assigns it via innerHTML" pattern
    /// [`sync_toolbar_state`] established. `downloads`/`history` are
    /// already-formatted display lines (the caller decides formatting,
    /// same as `side_panel_ui.rs`'s egui version does per-record).
    pub(crate) fn sync_downloads_history(&self, downloads: &[String], history: &[String]) {
        self.render_call("renderDownloads", &js_string_array(downloads));
        self.render_call("renderHistory", &js_string_array(history));
    }

    /// Rewrites `#error`'s text — used by the add-profile modal to reflect
    /// `AtomicApp::add_profile_error` every frame (safe to do every frame,
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
    /// `neutron::paint::list_adapters()`'s own order; `selected_adapter`/
    /// `credential_keys`/`bookmarks` mirror `AtomicApp`'s own state
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
        self.set_class_name(
            "fps-cap-off",
            if fps_cap.is_none() {
                "btn active"
            } else {
                "btn"
            },
        );
        self.set_class_name(
            "fps-cap-on",
            if fps_cap.is_some() {
                "btn active"
            } else {
                "btn"
            },
        );
        self.set_inner_html(
            "fps-value",
            &fps_cap
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
        );
        self.set_class_name(
            "keychain-off",
            if !use_keychain { "btn active" } else { "btn" },
        );
        self.set_class_name(
            "keychain-on",
            if use_keychain { "btn active" } else { "btn" },
        );

        // `settings.js`'s `renderAdapters` folds the old "active-if-none"
        // special case for the default button into a plain `active` prop —
        // `null` here is that prop's "none selected" sentinel.
        let selected_js = selected_adapter.map_or_else(|| "null".to_string(), |i| i.to_string());
        self.render_call(
            "renderAdapters",
            &format!("{},{selected_js}", js_string_array(adapters)),
        );

        self.render_call("renderCredentials", &js_string_array(credential_keys));

        self.set_inner_html("error", performance_error.unwrap_or(""));
        self.set_inner_html("vault-error", vault_error.unwrap_or(""));
        self.set_inner_html("import-result", &html_escape(import_result.unwrap_or("")));

        self.render_call("renderBookmarks", &js_string_array(bookmarks));
    }

    /// Calls a bundle-defined render function (e.g. `toolbar.js`'s
    /// `renderWorkspaces`) with a JS literal argument built by the caller
    /// (`js_workspace_array`, etc.) — the "Rust sends data, JS builds the
    /// component tree with `lib/ui.js`" counterpart to
    /// [`set_inner_html`](Self::set_inner_html)'s "Rust builds HTML, JS
    /// assigns it" pattern the string-based bundles still use.
    fn render_call(&self, fn_name: &str, args_js: &str) {
        let script = format!("{fn_name}({args_js});");
        let _ = self.ctx.eval(&script, "<chrome sync>");
    }

    /// Generic innerHTML rewrite for `#id` — the shared primitive both
    /// `sync_downloads_history` and `sync_settings_state` build on.
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
