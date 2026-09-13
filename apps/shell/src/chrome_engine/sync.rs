//! `sync_*`/`set_*` DOM-mutation methods — split out from
//! `chrome_engine.rs`.

use super::helpers::{html_escape, js_string_literal};
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
    /// `render::list_adapters()`'s own order; `selected_adapter`/
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
