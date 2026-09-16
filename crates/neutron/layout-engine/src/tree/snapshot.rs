//! `ElementSnapshot` construction (tag/attributes/classes/hover-focus/
//! sibling data) + `is_never_rendered`/`resolve_element_style` — split out
//! from `tree/mod.rs`.

use css::{matching_declarations_indexed, ElementSnapshot, SelectorIndex, Stylesheet};
use dom::{Dom, NodeData, NodeId};

use crate::style::{apply_inline_declarations, resolve_style, Color, ComputedStyle, FontFamily};

/// Tags a real browser's UA stylesheet gives `display: none` unconditionally
/// (their content is metadata/source text, never part of the rendered
/// page) — this engine has no UA stylesheet/tag-based default table at all
/// (see this module's other docs on that same gap for `display: inline`),
/// so without this check `<script>`/`<style>` source text and `<head>`
/// metadata would otherwise lay out and paint as ordinary visible content,
/// a real bug this closes (surfaced by a `profile-worker` test where a
/// `<style>` block placed before the element it styled visibly pushed that
/// element down the page). A page's own stylesheet can't override this by
/// setting `display: block` on one of these tags — real browsers don't
/// allow that either for `<head>`/`<script>`/`<style>` position in the
/// rendering process (a `<script>` unconditionally never renders, `display`
/// or not).
pub(super) fn is_never_rendered(tag: &str) -> bool {
    matches!(
        tag,
        "head" | "style" | "script" | "title" | "meta" | "link" | "base" | "noscript"
    )
}

fn classes_of(attributes: &std::collections::HashMap<String, String>) -> Vec<&str> {
    attributes
        .get("class")
        .map(|c| c.split_whitespace().collect())
        .unwrap_or_default()
}

/// Resolves `node`'s own style (pushing/matching/popping `chain`) — shared
/// by `build` (for a block-level element) and `collect_inline_spans` (for
/// an inline one), since both need the exact same cascade step.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_element_style<'a>(
    dom: &'a Dom,
    node: NodeId,
    tag: &str,
    attributes: &std::collections::HashMap<String, String>,
    sheet: &Stylesheet,
    index: &SelectorIndex,
    viewport_width: f64,
    viewport_height: f64,
    chain: &mut Vec<ElementSnapshot<'a>>,
    parent_font_size: f64,
    parent_color: Color,
    parent_font_family: FontFamily,
) -> ComputedStyle {
    let _ = tag; // already folded into build_element_snapshot's own dom lookup
    chain.push(build_element_snapshot(dom, node));
    let mut style = resolve_style(
        &matching_declarations_indexed(index, sheet, chain, viewport_width, viewport_height),
        parent_font_size,
        parent_color,
    );
    if let Some(inline) = attributes.get("style") {
        apply_inline_declarations(&mut style, &css::parse_inline_style(inline));
    }
    // Real `font-family` inheritance - see `ComputedStyle::font_family`'s
    // own doc for why this one layer up from `resolve_style` itself is
    // where it happens.
    if style.font_family.is_none() {
        style.font_family = Some(parent_font_family);
    }
    style
}

/// Real sibling/attribute data for `node`'s own [`ElementSnapshot`] entry
/// (attribute selectors, `+`/`~` combinators, `:first-child`/
/// `:last-child`/`:nth-child`) — derived from the same `dom::Dom` this
/// module already walks, not placeholder values. `preceding_siblings`
/// holds a full snapshot per earlier element sibling (skipping text/
/// comment nodes), each built the same recursive way, so a multi-hop
/// chain like `a + b + c` can walk from `c`'s own list into `b`'s own
/// nested list to reach `a` — see `css::cascade::ElementSnapshot`'s own
/// doc for how `selector_matches` consumes this. `node` with no parent
/// (the document root) or that isn't itself an element gets an empty/
/// default snapshot.
pub(super) fn build_element_snapshot(dom: &Dom, node: NodeId) -> ElementSnapshot<'_> {
    let Some(n) = dom.get(node) else {
        return ElementSnapshot::default();
    };
    let NodeData::Element {
        tag, attributes, ..
    } = &n.data
    else {
        return ElementSnapshot::default();
    };
    let (preceding_siblings, has_following_sibling) = sibling_snapshots(dom, node);
    ElementSnapshot {
        tag,
        id: attributes.get("id").map(String::as_str),
        classes: classes_of(attributes),
        attributes: attributes
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect(),
        preceding_siblings,
        has_following_sibling,
        is_hovered: dom.hovered_element() == Some(node),
        is_focused: dom.active_element() == Some(node),
    }
}

/// Element siblings (skipping text/comment nodes) sharing `node`'s parent:
/// every earlier one as a full recursive snapshot (document order,
/// immediately-preceding last, matching `ElementSnapshot::preceding_siblings`'s
/// own doc), plus whether at least one element sibling follows. `node`
/// with no parent (root) or that isn't found among its own parent's
/// children (shouldn't happen for a real `dom::Dom`, but this crate
/// doesn't panic on DOM inconsistency elsewhere either) gets `(empty,
/// false)`.
fn sibling_snapshots(dom: &Dom, node: NodeId) -> (Vec<ElementSnapshot<'_>>, bool) {
    let Some(parent) = dom.get(node).and_then(|n| n.parent) else {
        return (Vec::new(), false);
    };
    let Some(parent_node) = dom.get(parent) else {
        return (Vec::new(), false);
    };

    let element_siblings: Vec<NodeId> = parent_node
        .children
        .iter()
        .copied()
        .filter(|&c| matches!(dom.get(c).map(|n| &n.data), Some(NodeData::Element { .. })))
        .collect();

    let Some(pos) = element_siblings.iter().position(|&c| c == node) else {
        return (Vec::new(), false);
    };

    let preceding_siblings = element_siblings[..pos]
        .iter()
        .map(|&sib| build_element_snapshot(dom, sib))
        .collect();
    let has_following_sibling = pos + 1 < element_siblings.len();
    (preceding_siblings, has_following_sibling)
}
