//! `type_key` — split out from `input_commands.rs`.

use js_runtime::Context;

use super::helpers::js_string_literal;

/// Types `key` into whichever real `<input>`/`<textarea>` id `CLICK_AT`
/// most recently focused (see `dispatch_click_at`'s doc) - `"Backspace"`
/// removes the field's real last character, anything else is appended
/// verbatim as typed text. Writes the element's real `.value`
/// (`dom::Dom::value`/`set_value`) now, not `textContent` — a real
/// `"keydown"` event still dispatches first via the existing
/// `dispatchEvent` binding, so a page's own `keydown` listener genuinely
/// runs, same as a real browser firing the event before applying the
/// default action.
pub(crate) fn type_key(ctx: &Context, focused_id: &str, key: &str) -> Result<(), String> {
    let current = {
        let dom_ref = ctx.dom().ok_or("no DOM available")?;
        let node = dom_ref
            .find_by_id(focused_id)
            .ok_or_else(|| format!("no element with id \"{focused_id}\""))?;
        dom_ref.value(node)
    };
    let updated = if key == "Backspace" {
        let mut chars: Vec<char> = current.chars().collect();
        chars.pop();
        chars.into_iter().collect()
    } else {
        format!("{current}{key}")
    };
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.dispatchEvent(\"keydown\"); el.value = {value}; }})();",
        id = js_string_literal(focused_id),
        missing = js_string_literal(&format!("no element with id \"{focused_id}\"")),
        value = js_string_literal(&updated),
    );
    ctx.eval(&script, "<pane key>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{focused_id}\" (or its keydown handler threw)"))
}
