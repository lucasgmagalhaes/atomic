//! Focus/typing/input value reads — split out from `chrome_engine.rs`.

use super::helpers::js_string_literal;
use super::ChromeEngine;

impl<'rt> ChromeEngine<'rt> {
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
}
