//! Shared small helpers for the input-command handlers, split out from
//! `input_commands.rs`.

use dom::{Dom, NodeId};

/// Only `#id` selectors are supported for `CLICK`/`FILL` — this engine has
/// no CSS selector query beyond `dom::Dom::find_by_id` (no `css`-backed
/// `querySelector`, no class/attribute/descendant matching against a live
/// `Dom`). Rejects any other form up front rather than silently matching
/// nothing.
pub(super) fn require_id_selector(selector: &str) -> Result<&str, String> {
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
pub(super) fn js_string_literal(s: &str) -> String {
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

/// `true` if `node` is a real `<input>`/`<textarea>` — the only tags this
/// engine gives an independent `.value` (see `dom::Dom::value`'s own doc
/// on why that's exposed generically on `Node` rather than a typed
/// `HTMLInputElement`/`HTMLTextAreaElement` subclass).
pub(super) fn is_input_like(dom: &Dom, node: NodeId) -> bool {
    matches!(&dom.get(node).map(|n| &n.data), Some(dom::NodeData::Element { tag, .. }) if tag == "input" || tag == "textarea")
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
pub(super) fn nearest_id_ancestor(dom: &Dom, node: NodeId) -> Option<String> {
    let mut current = Some(node);
    while let Some(id) = current {
        if let Some(value) = dom.attribute(id, "id") {
            return Some(value.to_string());
        }
        current = dom.get(id).and_then(|n| n.parent);
    }
    None
}
