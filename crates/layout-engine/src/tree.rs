//! Box tree construction: walks a `dom::Dom` and attaches a resolved
//! `ComputedStyle` to every element, using `css`'s cascade against the
//! live ancestor chain. `dom::NodeData::Element` nodes become regular
//! boxes; `Text` nodes become boxes too, but carry their string in
//! `LayoutBox::text` instead of child boxes — actual glyph shaping/sizing
//! happens during layout (`layout::layout_children`), not here, since it
//! needs the containing width to wrap against. `Comment`/`Document` are
//! skipped. `display: none` prunes the whole subtree, matching the real
//! CSS box generation rule.
//!
//! Real inline formatting context now: consecutive `display: inline`
//! runs among a block/flex container's children — text nodes and
//! `display: inline` elements interleaved, e.g. `<p>hello <b>world</b>
//! today</p>` — get grouped into one synthetic inline box
//! (`LayoutBox::inline_spans`) instead of each becoming its own stacked
//! block. `collect_inline_spans` walks such a run's DOM subtree
//! recursively (an inline element can itself contain more text and
//! further inline elements), resolving each descendant's own cascaded
//! style so `<b>` in the example above keeps its own color/font-size as a
//! distinct span - shaped together with its neighbors as one paragraph by
//! `text::layout_inline` (real multi-span `cosmic-text` shaping, not
//! string concatenation). What's *not* modeled: a block-level element
//! appearing inside inline content (invalid per the HTML content model,
//! but real browsers "de-inline" it via anonymous block splitting - this
//! engine doesn't); this engine still only detects `display: inline` at
//! the point an inline run starts, not tag-based UA-stylesheet defaults,
//! so a stylesheet that wants `<b>`/`<span>`/etc. to flow inline has to
//! say so explicitly (`b, span { display: inline; }`) - there's no
//! built-in tag→display table anywhere in this crate.
use css::{matching_declarations, ElementSnapshot, Stylesheet};
use dom::{Dom, NodeData, NodeId};

use crate::style::{resolve_style, Color, ComputedStyle, Display};
use crate::text::PositionedGlyph;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Dimensions {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One already-styled run of text within a synthetic inline box
/// (`LayoutBox::inline_spans`) — see the module doc. Built by
/// `collect_inline_spans`, consumed by `text::layout_inline` (converted to
/// a borrowing `text::InlineSpan` right before shaping, since this owned
/// form is what survives across the box-tree/layout boundary).
#[derive(Debug, Clone)]
pub struct InlineSpanSource {
    pub text: String,
    pub font_size: f64,
    pub color: Color,
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub node: NodeId,
    pub style: ComputedStyle,
    pub children: Vec<LayoutBox>,
    pub dimensions: Dimensions,
    /// `Some` only for boxes built from a single `dom::NodeData::Text`
    /// node with no inline-element siblings next to it (the common "just
    /// text inside a block element" case). Mutually exclusive with
    /// `inline_spans` and non-empty `children`.
    pub text: Option<String>,
    /// `Some` only for a synthetic inline formatting context box grouping
    /// a run of text + `display: inline` element siblings — see the
    /// module doc. Mutually exclusive with `text` and non-empty
    /// `children`; `node` on a box like this identifies only its first
    /// source child (there's no single DOM node a merged inline run
    /// "is" — same idea as a real anonymous CSS box).
    pub inline_spans: Option<Vec<InlineSpanSource>>,
    /// Filled in by `layout::layout_children` once the text/inline box's
    /// width constraint is known; empty until then and always empty for
    /// boxes with real (non-text, non-inline-span) children.
    pub glyphs: Vec<PositionedGlyph>,
}

fn classes_of(attributes: &std::collections::HashMap<String, String>) -> Vec<String> {
    attributes
        .get("class")
        .map(|c| c.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Resolves `node`'s own style (pushing/matching/popping `chain`) — shared
/// by `build` (for a block-level element) and `collect_inline_spans` (for
/// an inline one), since both need the exact same cascade step.
fn resolve_element_style(
    dom: &Dom,
    node: NodeId,
    tag: &str,
    attributes: &std::collections::HashMap<String, String>,
    sheet: &Stylesheet,
    viewport_width: f64,
    chain: &mut Vec<ElementSnapshot>,
    parent_font_size: f64,
    parent_color: Color,
) -> ComputedStyle {
    let _ = (dom, node); // kept for signature symmetry/future use (e.g. sibling-position selectors)
    chain.push(ElementSnapshot {
        tag: tag.to_string(),
        id: attributes.get("id").cloned(),
        classes: classes_of(attributes),
    });
    resolve_style(&matching_declarations(sheet, chain, viewport_width), parent_font_size, parent_color)
}

/// Recursively flattens `node`'s subtree into `out` as [`InlineSpanSource`]
/// runs, for a node already known to be part of an inline formatting
/// context (a text node, or an element whose *own* resolved `display` is
/// `Inline`). Once inside such a run, every descendant contributes
/// inline — this doesn't re-check `display` on nested elements, per the
/// module doc's "block-inside-inline isn't de-inlined" scope cut.
fn collect_inline_spans(
    dom: &Dom,
    node: NodeId,
    sheet: &Stylesheet,
    viewport_width: f64,
    chain: &mut Vec<ElementSnapshot>,
    parent_font_size: f64,
    parent_color: Color,
    out: &mut Vec<InlineSpanSource>,
) {
    let Some(n) = dom.get(node) else { return };

    match &n.data {
        NodeData::Text(text) => {
            if !text.trim().is_empty() {
                out.push(InlineSpanSource {
                    text: text.clone(),
                    font_size: parent_font_size,
                    color: parent_color,
                });
            }
        }
        NodeData::Element { tag, attributes } => {
            let style = resolve_element_style(dom, node, tag, attributes, sheet, viewport_width, chain, parent_font_size, parent_color);
            if style.display != Display::None {
                for &child in &n.children {
                    collect_inline_spans(dom, child, sheet, viewport_width, chain, style.font_size, style.color, out);
                }
            }
            chain.pop();
        }
        NodeData::Comment(_) | NodeData::Document => {}
    }
}

/// `true` for a DOM child that starts/continues an inline run: a
/// non-whitespace text node, or an element whose own resolved `display`
/// is `Inline`. Resolving an element's style here means it gets resolved
/// again by whichever branch actually uses it (`build` or
/// `collect_inline_spans`) — cascade resolution is cheap enough (a handful
/// of declaration lookups) that recomputing it once more beats threading a
/// pre-resolved style through both call shapes.
fn is_inline_level(
    dom: &Dom,
    node: NodeId,
    sheet: &Stylesheet,
    viewport_width: f64,
    chain: &mut Vec<ElementSnapshot>,
    parent_font_size: f64,
    parent_color: Color,
) -> bool {
    match dom.get(node).map(|n| &n.data) {
        Some(NodeData::Text(text)) => !text.trim().is_empty(),
        Some(NodeData::Element { tag, attributes }) => {
            let style = resolve_element_style(dom, node, tag, attributes, sheet, viewport_width, chain, parent_font_size, parent_color);
            chain.pop();
            style.display == Display::Inline
        }
        _ => false,
    }
}

/// Builds the `LayoutBox` children of an element/root whose own children
/// list is `child_ids`, grouping consecutive inline-level runs (see
/// [`is_inline_level`]) into one synthetic inline box each rather than one
/// box per DOM child — the core of this module's inline-formatting-context
/// support.
fn build_children(
    dom: &Dom,
    child_ids: &[NodeId],
    sheet: &Stylesheet,
    viewport_width: f64,
    chain: &mut Vec<ElementSnapshot>,
    parent_font_size: f64,
    parent_color: Color,
) -> Vec<LayoutBox> {
    let mut result = Vec::new();
    let mut pending_inline_run: Vec<NodeId> = Vec::new();

    let flush = |run: &mut Vec<NodeId>, result: &mut Vec<LayoutBox>, chain: &mut Vec<ElementSnapshot>| {
        if run.is_empty() {
            return;
        }
        // A lone plain-text child (the overwhelmingly common case, and the
        // only shape this crate supported before inline formatting
        // contexts existed) takes the plain `text: Some(...)` path
        // unchanged - cheaper (skips rich-text shaping machinery for a
        // single span) and keeps `LayoutBox::text`'s existing contract for
        // callers/tests that only ever dealt with that case. Only an
        // actual multi-node or inline-element run needs the synthetic
        // `inline_spans` box.
        if run.len() == 1 && matches!(dom.get(run[0]).map(|n| &n.data), Some(NodeData::Text(_))) {
            let id = run[0];
            run.clear();
            if let Some(b) = build(dom, id, sheet, viewport_width, chain, parent_font_size, parent_color) {
                result.push(b);
            }
            return;
        }

        // Identifies the run by its first source node - see
        // `LayoutBox::inline_spans`'s doc on why this isn't semantically
        // meaningful beyond "some node in this run".
        let first_node = run[0];
        let mut spans = Vec::new();
        for &id in run.iter() {
            collect_inline_spans(dom, id, sheet, viewport_width, chain, parent_font_size, parent_color, &mut spans);
        }
        run.clear();
        if !spans.is_empty() {
            // The run's own baseline style, for the common single-span
            // case (plain text, no inline element siblings) where it's
            // exactly the text's real style - matches what a plain text
            // leaf box carried before this module supported multi-span
            // runs. When a run mixes multiple differently-styled spans,
            // this is just the run's inherited starting point, not
            // necessarily any one glyph's actual color/size - each glyph
            // still gets its own correct value via `inline_spans`.
            let mut style = ComputedStyle::initial();
            style.font_size = parent_font_size;
            style.color = parent_color;
            result.push(LayoutBox {
                node: first_node,
                style,
                children: Vec::new(),
                dimensions: Dimensions::default(),
                text: None,
                inline_spans: Some(spans),
                glyphs: Vec::new(),
            });
        }
    };

    for &child in child_ids {
        if is_inline_level(dom, child, sheet, viewport_width, chain, parent_font_size, parent_color) {
            pending_inline_run.push(child);
        } else {
            flush(&mut pending_inline_run, &mut result, chain);
            if let Some(b) = build(dom, child, sheet, viewport_width, chain, parent_font_size, parent_color) {
                result.push(b);
            }
        }
    }
    flush(&mut pending_inline_run, &mut result, chain);

    result
}

fn build(
    dom: &Dom,
    node: NodeId,
    sheet: &Stylesheet,
    viewport_width: f64,
    chain: &mut Vec<ElementSnapshot>,
    parent_font_size: f64,
    parent_color: Color,
) -> Option<LayoutBox> {
    let n = dom.get(node)?;

    if let NodeData::Text(text) = &n.data {
        if text.trim().is_empty() {
            return None;
        }
        let mut style = ComputedStyle::initial();
        style.font_size = parent_font_size;
        style.color = parent_color;
        return Some(LayoutBox {
            node,
            style,
            children: Vec::new(),
            dimensions: Dimensions::default(),
            text: Some(text.clone()),
            inline_spans: None,
            glyphs: Vec::new(),
        });
    }

    let NodeData::Element { tag, attributes } = &n.data else {
        return None;
    };

    chain.push(ElementSnapshot {
        tag: tag.clone(),
        id: attributes.get("id").cloned(),
        classes: classes_of(attributes),
    });
    let style = resolve_style(&matching_declarations(sheet, chain, viewport_width), parent_font_size, parent_color);

    let children = if style.display == Display::None {
        Vec::new()
    } else {
        build_children(dom, &n.children, sheet, viewport_width, chain, style.font_size, style.color)
    };
    chain.pop();

    if style.display == Display::None {
        return None;
    }

    Some(LayoutBox {
        node,
        style,
        children,
        dimensions: Dimensions::default(),
        text: None,
        inline_spans: None,
        glyphs: Vec::new(),
    })
}

/// A reasonable desktop-ish default for callers that don't care about
/// media queries (every existing call site before this crate supported
/// `@media` at all) — [`build_box_tree`] uses this; callers that actually
/// know their real viewport width (`profile-worker`, primarily) should
/// use [`build_box_tree_with_viewport`] instead so `(min-width: ...)`/
/// `(max-width: ...)` rules evaluate against the truth instead of a guess.
pub const DEFAULT_VIEWPORT_WIDTH: f64 = 1024.0;

/// Builds the box tree rooted at `node` (typically an `<html>`-equivalent
/// element, or any element for testing in isolation). Returns `None` if
/// `node` doesn't exist or isn't an element/non-empty-text node (includes
/// `display: none`, which produces no box at all per CSS box generation).
/// `node`'s inherited `font-size` starts at the CSS initial value (16px),
/// same as a real document root. Media queries evaluate against
/// [`DEFAULT_VIEWPORT_WIDTH`] — use [`build_box_tree_with_viewport`] to
/// pass a real one.
pub fn build_box_tree(dom: &Dom, node: NodeId, sheet: &Stylesheet) -> Option<LayoutBox> {
    build_box_tree_with_viewport(dom, node, sheet, DEFAULT_VIEWPORT_WIDTH)
}

/// Same as [`build_box_tree`], but resolves any `@media (min-width: ...)`/
/// `(max-width: ...)` conditions in `sheet` against a real `viewport_width`
/// instead of the arbitrary default.
pub fn build_box_tree_with_viewport(dom: &Dom, node: NodeId, sheet: &Stylesheet, viewport_width: f64) -> Option<LayoutBox> {
    let mut chain = Vec::new();
    let initial = ComputedStyle::initial();
    build(dom, node, sheet, viewport_width, &mut chain, initial.font_size, initial.color)
}
