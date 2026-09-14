//! `dispatch_mouse_move` — split out from `input_commands.rs`.

use dom::{Dom, NodeId};
use js_runtime::Context;

use crate::page::Page;

use super::helpers::js_string_literal;

/// Real coordinate-driven hover: hit-tests `(x, y)` via `Page::hit_test_at`
/// (same primitive `dispatch_click_at` already uses), updates `dom::Dom`'s
/// existing `:hover` state (`set_hovered`/`clear_hover` — real, already
/// wired to bump `style_version`, just never called from any host input
/// path before this), and — when the hovered element actually changes —
/// dispatches real bubbling `"mouseout"` on the previous target before a
/// real bubbling `"mouseover"` on the new one, via the nearest
/// id-addressable ancestor *element* of each (same real limitation
/// `input_commands::helpers::nearest_id_ancestor`'s own doc already gives
/// `CLICK_AT`). No-op (no dispatch at all) when the hit-tested node
/// resolves to the same id-addressable ancestor as before, matching a real
/// browser not re-firing `mouseover`/`mouseout` for movement within the
/// same element.
///
/// Hover is tracked by that ancestor *element*'s own `NodeId`, not the raw
/// hit-tested leaf — real, deliberate: a leaf hit-test can land on a text
/// node, and a `"mouseover"` listener that mutates its element's
/// `textContent` (a real, common pattern - see this file's own tests)
/// destroys that exact text node, which would otherwise make the next
/// `MOUSE_MOVE` see a stale `NodeId` and wrongly conclude nothing is
/// hovered anymore, re-dispatching `"mouseover"` on every subsequent move
/// within the same element. The ancestor element itself isn't destroyed by
/// a child-content-only mutation, so anchoring hover there is both more
/// correct (CSS `:hover` matches elements, never text nodes) and stable
/// across a listener's own DOM writes.
///
/// Scope cuts (`spec/matrix/events.md` line 17): `relatedTarget` stays
/// `null` on both events — `Context`'s only entry point is
/// `eval(source: &str)` (see `js_string_literal`'s own doc), so
/// cross-referencing two live JS element objects inside one generated
/// eval string isn't supported without a new binding. Non-bubbling
/// `mouseenter`/`mouseleave` are NOT dispatched here — they need an
/// ancestor-*set* diff, not a single-node dispatch.
pub(crate) fn dispatch_mouse_move(
    page: &mut Page,
    width: u32,
    height: u32,
    x: f64,
    y: f64,
    scroll_top: f64,
) -> Result<(), String> {
    let hit = page.hit_test_at(width, height, x, y, scroll_top);

    let previous_node = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        dom_ref.hovered_element()
    };
    let new_node = match hit {
        Some(node) => {
            let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
            nearest_id_ancestor_node(dom_ref, node)
        }
        None => None,
    };

    {
        let dom_mut = page.ctx.dom_mut().ok_or("no DOM available")?;
        match new_node {
            Some(node) => dom_mut.set_hovered(node),
            None => dom_mut.clear_hover(),
        }
    }

    if previous_node != new_node {
        if let Some(node) = previous_node {
            let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
            if let Some(id) = dom_ref.attribute(node, "id").map(str::to_string) {
                dispatch_hover_event(&page.ctx, &id, "mouseout")?;
            }
        }
        if let Some(node) = new_node {
            let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
            if let Some(id) = dom_ref.attribute(node, "id").map(str::to_string) {
                dispatch_hover_event(&page.ctx, &id, "mouseover")?;
            }
        }
    }
    Ok(())
}

/// Same walk `input_commands::helpers::nearest_id_ancestor` does, but
/// returns the ancestor element's own `NodeId` instead of its `id`
/// attribute string — this file's own doc above explains why hover state
/// needs the stable element id, not just the string.
fn nearest_id_ancestor_node(dom: &Dom, node: NodeId) -> Option<NodeId> {
    let mut current = Some(node);
    while let Some(id) = current {
        if dom.attribute(id, "id").is_some() {
            return Some(id);
        }
        current = dom.get(id).and_then(|n| n.parent);
    }
    None
}

fn dispatch_hover_event(ctx: &Context, id: &str, kind: &str) -> Result<(), String> {
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.dispatchEvent(new MouseEvent({kind}, {{ bubbles: true, cancelable: true }})); }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
        kind = js_string_literal(kind),
    );
    ctx.eval(&script, "<pane hover>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{id}\" (or its {kind} handler threw)"))
}
