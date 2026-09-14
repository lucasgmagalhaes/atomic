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
/// `CLICK_AT`) — plus real non-bubbling `"mouseleave"`/`"mouseenter"` on
/// every id-addressable ancestor that dropped out of / entered the hover
/// chain (see [`id_chain`]), same real "diff the whole ancestor set, not
/// just the nearest node" semantics a real browser uses for these two. No
/// dispatch at all when the hit-tested node resolves to the same
/// id-addressable ancestor as before, matching a real browser not
/// re-firing any of these four for movement within the same element.
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
/// Scope cut (`spec/matrix/events.md` line 17): `relatedTarget` stays
/// `null` on all four events — `Context`'s only entry point is
/// `eval(source: &str)` (see `js_string_literal`'s own doc), so
/// cross-referencing two live JS element objects inside one generated
/// eval string isn't supported without a new binding.
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
        let (old_chain, new_chain) = {
            let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
            (
                previous_node
                    .map(|n| id_chain(dom_ref, n))
                    .unwrap_or_default(),
                new_node.map(|n| id_chain(dom_ref, n)).unwrap_or_default(),
            )
        };

        if let Some(node) = previous_node {
            let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
            if let Some(id) = dom_ref.attribute(node, "id").map(str::to_string) {
                dispatch_hover_event(&page.ctx, &id, "mouseout", true)?;
            }
        }
        // Non-bubbling mouseleave: every id-addressable ancestor that was
        // in the old chain but isn't in the new one (real spec order -
        // innermost first - doesn't matter here since each dispatch is
        // target-phase-only, no bubbling to interact with).
        for (node, id) in &old_chain {
            if !new_chain.iter().any(|(n, _)| n == node) {
                dispatch_hover_event(&page.ctx, id, "mouseleave", false)?;
            }
        }

        if let Some(node) = new_node {
            let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
            if let Some(id) = dom_ref.attribute(node, "id").map(str::to_string) {
                dispatch_hover_event(&page.ctx, &id, "mouseover", true)?;
            }
        }
        // Non-bubbling mouseenter: symmetric - every id-addressable
        // ancestor newly in the hover chain.
        for (node, id) in &new_chain {
            if !old_chain.iter().any(|(n, _)| n == node) {
                dispatch_hover_event(&page.ctx, id, "mouseenter", false)?;
            }
        }
    }
    Ok(())
}

/// Walks every ancestor of `node` (including `node` itself) up to the
/// root, collecting each one that carries a real `id` attribute - unlike
/// `nearest_id_ancestor_node`, this doesn't stop at the first match, since
/// `mouseenter`/`mouseleave` needs the *whole* id-addressable ancestor
/// chain to diff against the previous hover's chain (`dispatch_mouse_move`
/// itself only ever anchors hover to the *nearest* one, so a caller
/// passing that anchor node in here still walks upward through every
/// further id'd ancestor above it).
fn id_chain(dom: &Dom, node: NodeId) -> Vec<(NodeId, String)> {
    let mut chain = Vec::new();
    let mut current = Some(node);
    while let Some(id) = current {
        if let Some(value) = dom.attribute(id, "id") {
            chain.push((id, value.to_string()));
        }
        current = dom.get(id).and_then(|n| n.parent);
    }
    chain
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

fn dispatch_hover_event(ctx: &Context, id: &str, kind: &str, bubbles: bool) -> Result<(), String> {
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.dispatchEvent(new MouseEvent({kind}, {{ bubbles: {bubbles}, cancelable: {bubbles} }})); }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
        kind = js_string_literal(kind),
        bubbles = bubbles,
    );
    ctx.eval(&script, "<pane hover>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{id}\" (or its {kind} handler threw)"))
}
