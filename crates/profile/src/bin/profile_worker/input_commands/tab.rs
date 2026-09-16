//! `TabOutcome`/`tab_focus` — split out from `input_commands.rs`.

use neutron::js::Context;

use crate::page::Page;

use super::focus::{blur_element, focus_element};
use super::helpers::js_string_literal;

/// Dispatches a real, cancelable `"keydown"` `KeyboardEvent` (`key: "Tab"`,
/// `shiftKey` set when `reverse`) at the currently focused `#id` element
/// (or `document` when nothing is focused, matching a real browser's
/// "focus starts on the document" default) — this is what lets a page's
/// own `keydown` listener call `preventDefault()` and actually suppress
/// the engine's default Tab-focus-movement, the same way
/// `neutron::js::dom_bindings::forms::run_default_click_action` already
/// respects `preventDefault()` on `"click"`. Returns `true` when the
/// default action was prevented.
fn dispatch_tab_keydown(
    ctx: &Context,
    focused_id: Option<&str>,
    reverse: bool,
) -> Result<bool, String> {
    let target_expr = match focused_id {
        Some(id) => format!("document.getElementById({})", js_string_literal(id)),
        None => "document".to_string(),
    };
    let script = format!(
        "(function(){{ var target = {target_expr}; if (target === null) throw {missing}; var evt = new KeyboardEvent(\"keydown\", {{ key: \"Tab\", bubbles: true, cancelable: true, shiftKey: {shift_key} }}); return !target.dispatchEvent(evt); }})();",
        target_expr = target_expr,
        missing = js_string_literal(&format!(
            "no element with id \"{}\"",
            focused_id.unwrap_or("")
        )),
        shift_key = reverse,
    );
    let result = ctx
        .eval(&script, "<pane tab keydown>")
        .map_err(|_| "the focused element's keydown handler threw".to_string())?;
    Ok(result == "true")
}

/// Outcome of [`tab_focus`] — split out from a plain `Option<String>` so
/// the caller (`profile_worker::main`'s `TAB`/`TAB_REVERSE` handler) can
/// report "the page's own `keydown` listener prevented this" separately
/// from "there's genuinely nothing to tab to", rather than collapsing
/// both into the same reply.
pub(crate) enum TabOutcome {
    Moved(String),
    NothingFocusable,
    DefaultPrevented,
}

/// Real `Tab` (`reverse: false`) / `Shift+Tab` (`reverse: true`) focus
/// movement — dispatches a real `"keydown"` first (see
/// `dispatch_tab_keydown`); if the page prevented it, no focus change
/// happens and this returns `Ok(TabOutcome::DefaultPrevented)`. Otherwise
/// computes the next target via `neutron::dom::Dom::next_focus_target`'s own
/// cycling rule, then blurs whatever was focused before and focuses the
/// new target through the real JS `.blur()`/`.focus()` bindings
/// (`blur_element`/`focus_element`, same as `dispatch_click_at`), so real
/// `"blur"`/`"change"`/`"focus"` events fire. Real limitation shared with
/// every other id-addressed command in this protocol (`CLICK`/`FILL`/
/// `CLICK_AT`): a tab-order target with no `id` attribute can't be
/// reached through a `document.getElementById`-based `eval()` script, so
/// this walks the tab order from the computed starting point in the same
/// direction (wrapping at most once around the whole list) until it finds
/// one with a real `id`, rather than silently landing on an unreachable
/// target. `Ok(TabOutcome::NothingFocusable)` when the tab order is empty
/// or every element in it lacks an `id` - both report as "nothing to tab
/// to" to the caller, same as `hit_test_at`'s "nothing there" convention
/// for `CLICK_AT`.
pub(crate) fn tab_focus(page: &mut Page, reverse: bool) -> Result<TabOutcome, String> {
    let previously_focused_id_for_keydown = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        dom_ref
            .active_element()
            .and_then(|prev| dom_ref.attribute(prev, "id").map(str::to_string))
    };
    if dispatch_tab_keydown(
        &page.ctx,
        previously_focused_id_for_keydown.as_deref(),
        reverse,
    )? {
        return Ok(TabOutcome::DefaultPrevented);
    }
    let (target_id, previously_focused_id) = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        let order = dom_ref.tab_order();
        if order.is_empty() {
            return Ok(TabOutcome::NothingFocusable);
        }
        let current_index = dom_ref
            .active_element()
            .and_then(|id| order.iter().position(|&n| n == id));
        let start = match (current_index, reverse) {
            (Some(i), false) => (i + 1) % order.len(),
            (Some(i), true) => (i + order.len() - 1) % order.len(),
            (None, false) => 0,
            (None, true) => order.len() - 1,
        };
        let step: i64 = if reverse { -1 } else { 1 };
        let target_id = (0..order.len()).find_map(|offset| {
            let index =
                (start as i64 + step * offset as i64).rem_euclid(order.len() as i64) as usize;
            dom_ref.attribute(order[index], "id").map(str::to_string)
        });
        let previously_focused_id = dom_ref
            .active_element()
            .and_then(|prev| dom_ref.attribute(prev, "id").map(str::to_string));
        (target_id, previously_focused_id)
    };

    let Some(target_id) = target_id else {
        return Ok(TabOutcome::NothingFocusable);
    };
    if let Some(prev_id) = previously_focused_id {
        if prev_id != target_id {
            let _ = blur_element(&page.ctx, &prev_id);
        }
    }
    focus_element(&page.ctx, &target_id)?;
    Ok(TabOutcome::Moved(target_id))
}
