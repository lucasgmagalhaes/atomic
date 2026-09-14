//! `dispatch_composition_start`/`dispatch_composition_update`/
//! `dispatch_composition_end` — split out from `input_commands.rs`. Real
//! IME composition surface: operates on whichever id `CLICK_AT`/`KEY`
//! most recently focused (see `dispatch_click_at`'s doc), same convention
//! `type_key` already uses.

use js_runtime::Context;

use super::helpers::js_string_literal;

/// Real `"compositionstart"`: bubbles, cancelable (real spec shape),
/// `data: ""`. No field mutation - a real IME session starting has no
/// text yet.
pub(crate) fn dispatch_composition_start(ctx: &Context, focused_id: &str) -> Result<(), String> {
    dispatch(ctx, focused_id, "compositionstart", "", true, true)
}

/// Real `"compositionupdate"`: bubbles, not cancelable, `data: text`. No
/// field mutation either - matches a real IME only *previewing*
/// candidate text mid-composition, committing nothing until
/// `compositionend`.
pub(crate) fn dispatch_composition_update(
    ctx: &Context,
    focused_id: &str,
    text: &str,
) -> Result<(), String> {
    dispatch(ctx, focused_id, "compositionupdate", text, true, false)
}

/// Real `"compositionend"`: bubbles, not cancelable (real spec: this one
/// can't be prevented, unlike this file's siblings' cancelable-gated
/// defaults elsewhere in this crate). The real default action always
/// runs afterward: appends `text` to the field's `.value` (append, not
/// cursor-position insert - same convention `type_key`'s own regular
/// character handling uses). That assignment itself already dispatches a
/// real (generic, not `InputEvent`-typed) `"input"` event -
/// `dom_bindings::content::text_value`'s own `node_value_set` fires one
/// on every `.value` mutation, the same real mechanism `type_key`'s own
/// `el.value = ...` assignment already relies on for regular typed
/// characters - so this doesn't build a second, richer `InputEvent` on
/// top; that would just be a redundant, spec-inaccurate double dispatch.
pub(crate) fn dispatch_composition_end(
    ctx: &Context,
    focused_id: &str,
    text: &str,
) -> Result<(), String> {
    dispatch(ctx, focused_id, "compositionend", text, true, false)?;

    let current = {
        let dom_ref = ctx.dom().ok_or("no DOM available")?;
        let node = dom_ref
            .find_by_id(focused_id)
            .ok_or_else(|| format!("no element with id \"{focused_id}\""))?;
        dom_ref.value(node)
    };
    let updated = format!("{current}{text}");
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.value = {value}; }})();",
        id = js_string_literal(focused_id),
        missing = js_string_literal(&format!("no element with id \"{focused_id}\"")),
        value = js_string_literal(&updated),
    );
    ctx.eval(&script, "<pane composition>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{focused_id}\" (or its input handler threw)"))
}

fn dispatch(
    ctx: &Context,
    focused_id: &str,
    kind: &str,
    data: &str,
    bubbles: bool,
    cancelable: bool,
) -> Result<(), String> {
    let script = format!(
        "(function(){{ \
            var el = document.getElementById({id}); \
            if (el === null) throw {missing}; \
            el.dispatchEvent(new CompositionEvent({kind}, {{ bubbles: {bubbles}, cancelable: {cancelable}, data: {data} }})); \
        }})();",
        id = js_string_literal(focused_id),
        missing = js_string_literal(&format!("no element with id \"{focused_id}\"")),
        kind = js_string_literal(kind),
        data = js_string_literal(data),
        bubbles = bubbles,
        cancelable = cancelable,
    );
    ctx.eval(&script, "<pane composition>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{focused_id}\" (or its {kind} handler threw)"))
}
