//! `fill_element` — split out from `input_commands.rs`.

use neutron::js::Context;

use super::helpers::{is_input_like, js_string_literal, require_id_selector};

/// Sets the `#id` element's `.value` (a real, independent `neutron::dom::Dom`
/// property now — see `is_input_like`/`neutron::dom::Dom::value`) if it's an
/// `<input>`/`<textarea>`, `textContent` otherwise (this engine's only
/// settable string for a generic element).
pub(crate) fn fill_element(ctx: &Context, selector: &str, value: &str) -> Result<(), String> {
    let id = require_id_selector(selector)?;
    let prop = {
        let dom_ref = ctx.dom().ok_or("no DOM available")?;
        let node = dom_ref
            .find_by_id(id)
            .ok_or_else(|| format!("no element with id \"{id}\""))?;
        if is_input_like(dom_ref, node) {
            "value"
        } else {
            "textContent"
        }
    };
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.{prop} = {value}; }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
        value = js_string_literal(value),
    );
    ctx.eval(&script, "<pane fill>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{id}\""))
}
