//! `dispatch_context_menu_at` — split out from `input_commands.rs`.

use js_runtime::Context;

use crate::page::Page;

use super::helpers::{js_string_literal, nearest_id_ancestor};

/// Real coordinate-driven right-click: hit-tests `(x, y)` via
/// `Page::hit_test_at` (same primitive `CLICK_AT`/`MOUSE_MOVE` use) and
/// dispatches a real bubbling, cancelable `"contextmenu"` `MouseEvent`
/// (`button: 2`, matching a real right-click) on the nearest
/// id-addressable ancestor (same real limitation
/// `input_commands::helpers::nearest_id_ancestor`'s own doc already gives
/// `CLICK_AT`).
///
/// Scope cut: unlike `CLICK_AT`, this never moves focus - a real
/// right-click's focus effect is platform-dependent, and there's no
/// native context menu in this engine to model that behavior against (no
/// default action to suppress via `preventDefault()` either - the event
/// itself, and whatever a page's own listener does with it, is the real
/// surface here).
pub(crate) fn dispatch_context_menu_at(
    page: &mut Page,
    width: u32,
    height: u32,
    x: f64,
    y: f64,
    scroll_top: f64,
) -> Result<(), String> {
    let node = page
        .hit_test_at(width, height, x, y, scroll_top)
        .ok_or("no element at that point")?;
    let id = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        nearest_id_ancestor(dom_ref, node)
            .ok_or("no id-addressable element at or above that point")?
    };
    dispatch_context_menu(&page.ctx, &id)
}

fn dispatch_context_menu(ctx: &Context, id: &str) -> Result<(), String> {
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.dispatchEvent(new MouseEvent(\"contextmenu\", {{ bubbles: true, cancelable: true, button: 2 }})); }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
    );
    ctx.eval(&script, "<pane contextmenu>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{id}\" (or its contextmenu handler threw)"))
}
