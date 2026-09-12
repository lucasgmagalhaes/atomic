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
use css::{
    build_selector_index, matching_declarations_indexed, ElementSnapshot, SelectorIndex, Stylesheet,
};
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

/// Real resolved (pixel) border thickness per side, set by
/// `layout::layout_block` once it knows `containing_width` — `style.
/// border_width` alone (still a `Length`, possibly a percentage) isn't
/// enough for a renderer to paint a border stroke without redoing that
/// resolution itself. All-zero (the `Default`) for a box whose
/// `border_style` is `None`, same as `layout_block`'s own box-model math
/// already treats it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ResolvedBorder {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
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
    /// See [`ResolvedBorder`]. Set by `layout::layout_block`; zero for
    /// every box until layout actually runs.
    pub border: ResolvedBorder,
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
    /// `Some` only for an `<img>` element box whose image was actually
    /// fetched and decoded successfully (a caller supplies this via
    /// [`apply_image_sizes`] — box tree construction itself never fetches
    /// anything over the network). `None` for every other box, and for
    /// an `<img>` with no `src`, a failed fetch, or undecodable bytes -
    /// those all render as an empty box, same as this engine already
    /// treats an unknown/unstyled element.
    pub image: Option<std::rc::Rc<image_decode::DecodedImage>>,
}

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
fn is_never_rendered(tag: &str) -> bool {
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
fn resolve_element_style<'a>(
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
) -> ComputedStyle {
    let _ = (tag, attributes); // already folded into build_element_snapshot's own dom lookup
    chain.push(build_element_snapshot(dom, node));
    resolve_style(
        &matching_declarations_indexed(index, sheet, chain, viewport_width, viewport_height),
        parent_font_size,
        parent_color,
    )
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
fn build_element_snapshot(dom: &Dom, node: NodeId) -> ElementSnapshot<'_> {
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

/// Recursively flattens `node`'s subtree into `out` as [`InlineSpanSource`]
/// runs, for a node already known to be part of an inline formatting
/// context (a text node, or an element whose *own* resolved `display` is
/// `Inline`). Once inside such a run, every descendant contributes
/// inline — this doesn't re-check `display` on nested elements, per the
/// module doc's "block-inside-inline isn't de-inlined" scope cut.
fn collect_inline_spans<'a>(
    dom: &'a Dom,
    node: NodeId,
    sheet: &Stylesheet,
    index: &SelectorIndex,
    viewport_width: f64,
    viewport_height: f64,
    chain: &mut Vec<ElementSnapshot<'a>>,
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
        NodeData::Element {
            tag, attributes, ..
        } => {
            if is_never_rendered(tag) {
                return;
            }
            let style = resolve_element_style(
                dom,
                node,
                tag,
                attributes,
                sheet,
                index,
                viewport_width,
                viewport_height,
                chain,
                parent_font_size,
                parent_color,
            );
            if style.display != Display::None {
                for &child in &n.children {
                    collect_inline_spans(
                        dom,
                        child,
                        sheet,
                        index,
                        viewport_width,
                        viewport_height,
                        chain,
                        style.font_size,
                        style.color,
                        out,
                    );
                }
            }
            chain.pop();
        }
        NodeData::Comment(_) | NodeData::Document | NodeData::DocumentFragment => {}
    }
}

/// `true` for a DOM child that starts/continues an inline run: a
/// non-whitespace text node, or an element whose own resolved `display`
/// is `Inline`. Resolving an element's style here means it gets resolved
/// again by whichever branch actually uses it (`build` or
/// `collect_inline_spans`) — cascade resolution is cheap enough (a handful
/// of declaration lookups) that recomputing it once more beats threading a
/// pre-resolved style through both call shapes.
fn is_inline_level<'a>(
    dom: &'a Dom,
    node: NodeId,
    sheet: &Stylesheet,
    index: &SelectorIndex,
    viewport_width: f64,
    viewport_height: f64,
    chain: &mut Vec<ElementSnapshot<'a>>,
    parent_font_size: f64,
    parent_color: Color,
) -> bool {
    match dom.get(node).map(|n| &n.data) {
        Some(NodeData::Text(text)) => !text.trim().is_empty(),
        Some(NodeData::Element {
            tag, attributes, ..
        }) => {
            if is_never_rendered(tag) {
                return false;
            }
            let style = resolve_element_style(
                dom,
                node,
                tag,
                attributes,
                sheet,
                index,
                viewport_width,
                viewport_height,
                chain,
                parent_font_size,
                parent_color,
            );
            chain.pop();
            style.display == Display::Inline
        }
        _ => false,
    }
}

/// Builds every child box of `node` in one pass, grouping consecutive
/// inline-level children (see [`is_inline_level`]) into a single
/// [`InlineSpanSource`]-carrying box instead of one box per child — the
/// real inline formatting context this module's doc describes. A run of
/// exactly one plain-text child (no inline element siblings) still takes
/// the cheaper pre-existing `text: Some(...)` path via `build` instead,
/// unchanged from before multi-span runs existed.
fn build_children<'a>(
    dom: &'a Dom,
    child_ids: &[NodeId],
    sheet: &Stylesheet,
    index: &SelectorIndex,
    viewport_width: f64,
    viewport_height: f64,
    chain: &mut Vec<ElementSnapshot<'a>>,
    parent_font_size: f64,
    parent_color: Color,
) -> Vec<LayoutBox> {
    let mut result = Vec::new();
    let mut pending_inline_run: Vec<NodeId> = Vec::new();

    let flush = |run: &mut Vec<NodeId>,
                 result: &mut Vec<LayoutBox>,
                 chain: &mut Vec<ElementSnapshot<'a>>| {
        if run.is_empty() {
            return;
        }
        if run.len() == 1 {
            if let Some(NodeData::Text(_)) = dom.get(run[0]).map(|n| &n.data) {
                if let Some(b) = build(
                    dom,
                    run[0],
                    sheet,
                    index,
                    viewport_width,
                    viewport_height,
                    chain,
                    parent_font_size,
                    parent_color,
                ) {
                    result.push(b);
                }
                run.clear();
                return;
            }
        }

        let mut spans = Vec::new();
        let first_node = run[0];
        for &id in run.iter() {
            collect_inline_spans(
                dom,
                id,
                sheet,
                index,
                viewport_width,
                viewport_height,
                chain,
                parent_font_size,
                parent_color,
                &mut spans,
            );
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
                border: ResolvedBorder::default(),
                text: None,
                inline_spans: Some(spans),
                glyphs: Vec::new(),
                image: None,
            });
        }
    };

    for &child in child_ids {
        if is_inline_level(
            dom,
            child,
            sheet,
            index,
            viewport_width,
            viewport_height,
            chain,
            parent_font_size,
            parent_color,
        ) {
            pending_inline_run.push(child);
        } else {
            flush(&mut pending_inline_run, &mut result, chain);
            if let Some(b) = build(
                dom,
                child,
                sheet,
                index,
                viewport_width,
                viewport_height,
                chain,
                parent_font_size,
                parent_color,
            ) {
                result.push(b);
            }
        }
    }
    flush(&mut pending_inline_run, &mut result, chain);

    result
}

fn build<'a>(
    dom: &'a Dom,
    node: NodeId,
    sheet: &Stylesheet,
    index: &SelectorIndex,
    viewport_width: f64,
    viewport_height: f64,
    chain: &mut Vec<ElementSnapshot<'a>>,
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
            border: ResolvedBorder::default(),
            text: Some(text.clone()),
            inline_spans: None,
            glyphs: Vec::new(),
            image: None,
        });
    }

    let NodeData::Element { tag, .. } = &n.data else {
        return None;
    };
    if is_never_rendered(tag) {
        return None;
    }

    chain.push(build_element_snapshot(dom, node));
    let style = resolve_style(
        &matching_declarations_indexed(index, sheet, chain, viewport_width, viewport_height),
        parent_font_size,
        parent_color,
    );

    let children = if style.display == Display::None {
        Vec::new()
    } else {
        build_children(
            dom,
            &n.children,
            sheet,
            index,
            viewport_width,
            viewport_height,
            chain,
            style.font_size,
            style.color,
        )
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
        border: ResolvedBorder::default(),
        text: None,
        inline_spans: None,
        glyphs: Vec::new(),
        image: None,
    })
}

/// Real, viewport-width-agnostic box tree construction - defaults to a
/// hardcoded 1280px viewport (see [`DEFAULT_VIEWPORT_WIDTH`]) for any
/// `@media` rule in `sheet` that references one, since the caller hasn't
/// supplied a real one. Prefer [`build_box_tree_with_viewport`] whenever
/// the caller has a real viewport width on hand.
pub const DEFAULT_VIEWPORT_WIDTH: f64 = 1280.0;
/// Paired default for [`build_box_tree`] — see [`DEFAULT_VIEWPORT_WIDTH`]'s
/// own doc; an arbitrary but plausible desktop-viewport height for any
/// `@media (min-height: ...)`/`(max-height: ...)` rule when the caller
/// hasn't supplied a real one.
pub const DEFAULT_VIEWPORT_HEIGHT: f64 = 800.0;

pub fn build_box_tree(dom: &Dom, node: NodeId, sheet: &Stylesheet) -> Option<LayoutBox> {
    build_box_tree_with_viewport(
        dom,
        node,
        sheet,
        DEFAULT_VIEWPORT_WIDTH,
        DEFAULT_VIEWPORT_HEIGHT,
    )
}

/// Same as [`build_box_tree`], but resolves any `@media (min-width: ...)`/
/// `(max-width: ...)`/`(min-height: ...)`/`(max-height: ...)` conditions in
/// `sheet` against a real `viewport_width`/`viewport_height` instead of the
/// arbitrary defaults.
pub fn build_box_tree_with_viewport(
    dom: &Dom,
    node: NodeId,
    sheet: &Stylesheet,
    viewport_width: f64,
    viewport_height: f64,
) -> Option<LayoutBox> {
    let mut chain = Vec::new();
    let initial = ComputedStyle::initial();
    let index = build_selector_index(sheet);
    build(
        dom,
        node,
        sheet,
        &index,
        viewport_width,
        viewport_height,
        &mut chain,
        initial.font_size,
        initial.color,
    )
}

/// Applies real fetched/decoded `<img>` images onto an already-built box
/// tree: sets [`LayoutBox::image`] and, for any `<img>` box whose CSS
/// `width`/`height` weren't explicitly set (`Length::Auto`), overrides
/// them with the image's real intrinsic pixel dimensions - real
/// intrinsic sizing, achieved by feeding an ordinary `Length::Px` into
/// the exact same `layout::layout_block` code path an explicit
/// `width: 200px` would use (no changes to `layout.rs` itself needed).
/// An `<img>` with an explicit CSS width/height keeps it, matching real
/// browsers (CSS always wins over intrinsic size when both are present).
///
/// Must run after [`build_box_tree`]/[`build_box_tree_with_viewport`] and
/// before `layout::layout_block` — box tree construction itself never
/// fetches anything over the network. `images` is supplied by the
/// caller (e.g. `profile-worker`, after a `net::get` + `image_decode::
/// decode` per `<img src>` found in the page), keyed by the `<img>`
/// element's own `NodeId`. A node with no entry (no `src`, a failed
/// fetch, undecodable bytes) is left as a plain empty box, same as any
/// other unstyled element.
pub fn apply_image_sizes(
    dom: &Dom,
    box_: &mut LayoutBox,
    images: &std::collections::HashMap<NodeId, std::rc::Rc<image_decode::DecodedImage>>,
) {
    if let Some(dom::Node {
        data: NodeData::Element { tag, .. },
        ..
    }) = dom.get(box_.node)
    {
        if tag.eq_ignore_ascii_case("img") {
            if let Some(image) = images.get(&box_.node) {
                if box_.style.width == crate::style::Length::Auto {
                    box_.style.width = crate::style::Length::Px(image.width as f64);
                }
                if box_.style.height == crate::style::Length::Auto {
                    box_.style.height = crate::style::Length::Px(image.height as f64);
                }
                box_.image = Some(image.clone());
            }
        }
    }
    for child in &mut box_.children {
        apply_image_sizes(dom, child, images);
    }
}
