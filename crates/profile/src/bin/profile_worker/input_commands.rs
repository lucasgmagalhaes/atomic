//! Simulating user input against a live page: clicks (by selector or by
//! coordinate), focus/blur, typing, tabbing, and filling form-like
//! elements.

use dom::{Dom, NodeId};
use js_runtime::Context;

use crate::page::Page;

/// Only `#id` selectors are supported for `CLICK`/`FILL` — this engine has
/// no CSS selector query beyond `dom::Dom::find_by_id` (no `css`-backed
/// `querySelector`, no class/attribute/descendant matching against a live
/// `Dom`). Rejects any other form up front rather than silently matching
/// nothing.
fn require_id_selector(selector: &str) -> Result<&str, String> {
    selector
        .strip_prefix('#')
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            format!("unsupported selector \"{selector}\" - only #id selectors are implemented")
        })
}

/// A valid JS double-quoted string literal for `s` - just enough escaping
/// to safely embed an arbitrary Rust string (an id, an error message, a
/// fill value) into the small JS snippets `dispatch_click`/`fill_element`
/// build and eval, without a real `JS_NewStringLen`-based binding for
/// "call this method with this string argument" existing yet (the only
/// public entry point into a `Context` is `eval(source)` - see that
/// method's own doc).
fn js_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

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

/// Real focus, via the JS `.focus()` binding rather than calling
/// `dom::Dom::focus` directly — routing through JS means the real
/// `"focus"` event dispatches too (`dom_bindings::node_focus` calls
/// `events::dispatch` after mutating `dom::Dom`'s focus state), same
/// reasoning `dispatch_click` already applies to clicks.
fn focus_element(ctx: &Context, id: &str) -> Result<(), String> {
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
/// for why this goes through JS instead of `dom::Dom::blur` directly:
/// `dom_bindings::node_blur` dispatches a real `"blur"` event, and a real
/// `"change"` event too if `.value` moved since the matching `focus()`.
fn blur_element(ctx: &Context, id: &str) -> Result<(), String> {
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.blur(); }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
    );
    ctx.eval(&script, "<pane blur>")
        .map(|_| ())
        .map_err(|_| format!("no element with id \"{id}\""))
}

/// `true` if `node` is a real `<input>`/`<textarea>` — the only tags this
/// engine gives an independent `.value` (see `dom::Dom::value`'s own doc
/// on why that's exposed generically on `Node` rather than a typed
/// `HTMLInputElement`/`HTMLTextAreaElement` subclass).
fn is_input_like(dom: &Dom, node: NodeId) -> bool {
    matches!(&dom.get(node).map(|n| &n.data), Some(dom::NodeData::Element { tag, .. }) if tag == "input" || tag == "textarea")
}

/// Sets the `#id` element's `.value` (a real, independent `dom::Dom`
/// property now — see `is_input_like`/`dom::Dom::value`) if it's an
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

/// Walks from `node` up through real `dom::Dom` parent links (not a JS
/// call — this runs before any JS-level dispatch happens) looking for the
/// nearest ancestor (including `node` itself) that has a real `id`
/// attribute. `CLICK_AT`'s real limitation, same as automation's existing
/// `#id`-only `CLICK`/`FILL` (see `require_id_selector`'s own doc): a
/// coordinate click can only ever reach an element the page itself gave
/// an id to, directly or via one of its ancestors — real event bubbling
/// still finds *a* real listener target this way in the common case
/// (buttons/links typically carry an id, or a parent does), but a click
/// on a page with no ids anywhere genuinely can't be dispatched, and this
/// returns `None` rather than silently no-opping past that.
fn nearest_id_ancestor(dom: &Dom, node: NodeId) -> Option<String> {
    let mut current = Some(node);
    while let Some(id) = current {
        if let Some(value) = dom.attribute(id, "id") {
            return Some(value.to_string());
        }
        current = dom.get(id).and_then(|n| n.parent);
    }
    None
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

/// Dispatches a real, cancelable `"keydown"` `KeyboardEvent` (`key: "Tab"`,
/// `shiftKey` set when `reverse`) at the currently focused `#id` element
/// (or `document` when nothing is focused, matching a real browser's
/// "focus starts on the document" default) — this is what lets a page's
/// own `keydown` listener call `preventDefault()` and actually suppress
/// the engine's default Tab-focus-movement, the same way
/// `js_runtime::dom_bindings::forms::run_default_click_action` already
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
/// computes the next target via `dom::Dom::next_focus_target`'s own
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
