//! `dispatch_click`/`dispatch_click_at` — split out from `input_commands.rs`.

use js_runtime::Context;

use crate::page::Page;

use super::focus::{blur_element, focus_element};
use super::helpers::{is_input_like, js_string_literal, nearest_id_ancestor, require_id_selector};

/// Dispatches a real `"click"` event at the `#id` element via the
/// already-real `Node.prototype.dispatchEvent` JS binding - if the page's
/// own script attached a `"click"` listener via `addEventListener`, it
/// actually runs. `Err` covers both "no such id" and the listener itself
/// throwing - `js_runtime::Context::eval`'s own placeholder exception
/// message (see its doc comment) can't currently distinguish the two, so
/// this reports the more actionable one.
pub(crate) fn dispatch_click(ctx: &Context, selector: &str) -> Result<(), String> {
    let id = require_id_selector(selector)?;
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.dispatchEvent(\"click\"); }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
    );
    ctx.eval(&script, "<pane click>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{id}\" (or its click handler threw)"))
}

/// Real coordinate click: hit-tests `(x, y)`, dispatches a real `"click"`
/// via the nearest id-addressable ancestor (see `nearest_id_ancestor`'s
/// own doc for the real limitation that implies), and — real focus model,
/// via `focus_element`/`blur_element` (JS `.focus()`/`.blur()`, not
/// `dom::Dom::focus`/`clear_focus` called directly, so real `"focus"`/
/// `"blur"`/`"change"` events dispatch too) — focuses the hit node itself
/// if it's a real `<input>`/`<textarea>` with its own `id` (so `KEY` can
/// type into it, and `document.activeElement` sees it too), or clears
/// focus otherwise (matches a real browser blurring whatever was focused
/// when a click lands somewhere non-focusable). Whatever was focused
/// before is blurred first, same order a real browser fires them in
/// (`blur` on the old element before `focus` on the new one). A click that
/// lands back on the already-focused element is a no-op for focus (no
/// redundant `blur`+`focus` pair), matching a real browser not re-firing
/// focus on a click to a field that already has it. See `tab_focus` for
/// the other way focus moves - a real `Tab`/`Shift+Tab` press.
pub(crate) fn dispatch_click_at(
    page: &mut Page,
    width: u32,
    height: u32,
    x: f64,
    y: f64,
    scroll_top: f64,
) -> Result<Option<String>, String> {
    let node = page
        .hit_test_at(width, height, x, y, scroll_top)
        .ok_or("no element at that point")?;
    let click_id = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        nearest_id_ancestor(dom_ref, node)
            .ok_or("no id-addressable element at or above that point")?
    };
    dispatch_click(&page.ctx, &format!("#{click_id}"))?;

    // Re-borrow fresh rather than reusing the pre-dispatch `dom_ref` - the
    // click listener that just ran (real JS, via `dispatch_click` above)
    // could have mutated the DOM, and this engine's arena (`dom::Dom`'s
    // internal `Vec<Slot>`) isn't guaranteed not to reallocate on a node
    // creation in between.
    let (focusable, focus_id, already_focused, previously_focused_id) = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        let focusable = is_input_like(dom_ref, node);
        let focus_id = if focusable {
            dom_ref.attribute(node, "id").map(str::to_string)
        } else {
            None
        };
        let already_focused = focusable && dom_ref.active_element() == Some(node);
        let previously_focused_id = if already_focused {
            None
        } else {
            dom_ref
                .active_element()
                .and_then(|prev| dom_ref.attribute(prev, "id").map(str::to_string))
        };
        (focusable, focus_id, already_focused, previously_focused_id)
    };
    if let Some(prev_id) = previously_focused_id {
        let _ = blur_element(&page.ctx, &prev_id);
    }
    if focusable && !already_focused {
        if let Some(id) = &focus_id {
            let _ = focus_element(&page.ctx, id);
        }
    }
    Ok(focus_id)
}
