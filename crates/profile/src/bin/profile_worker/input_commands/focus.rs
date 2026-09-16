//! Real focus/blur via the JS `.focus()`/`.blur()` bindings — split out
//! from `input_commands.rs`.

use neutron::js::Context;

use super::helpers::js_string_literal;

/// Real focus, via the JS `.focus()` binding rather than calling
/// `neutron::dom::Dom::focus` directly — routing through JS means the real
/// `"focus"` event dispatches too (`dom_bindings::node_focus` calls
/// `events::dispatch` after mutating `neutron::dom::Dom`'s focus state), same
/// reasoning `dispatch_click` already applies to clicks.
pub(super) fn focus_element(ctx: &Context, id: &str) -> Result<(), String> {
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.focus(); }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
    );
    ctx.eval(&script, "<pane focus>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{id}\""))
}

/// Real blur, via the JS `.blur()` binding — see `focus_element`'s own doc
/// for why this goes through JS instead of `neutron::dom::Dom::blur` directly:
/// `dom_bindings::node_blur` dispatches a real `"blur"` event, and a real
/// `"change"` event too if `.value` moved since the matching `focus()`.
pub(super) fn blur_element(ctx: &Context, id: &str) -> Result<(), String> {
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.blur(); }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
    );
    ctx.eval(&script, "<pane blur>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{id}\""))
}
