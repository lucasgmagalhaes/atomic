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
//!
//! Split into `types.rs` (`Dimensions`/`ResolvedBorder`/
//! `InlineSpanSource`/`LayoutBox`), `snapshot.rs` (`ElementSnapshot`
//! construction + `is_never_rendered`/`resolve_element_style`),
//! `inline.rs` (`collect_inline_spans`/`is_inline_level`), and `build.rs`
//! (the recursive `build_children`/`build`) — this file keeps the public
//! entry points: `build_box_tree`, `build_box_tree_with_viewport`, and
//! `apply_image_sizes`.

use css::{build_selector_index, Stylesheet};
use dom::{Dom, NodeData, NodeId};

use crate::style::ComputedStyle;

mod build;
mod inline;
mod snapshot;
mod types;

pub use types::{Dimensions, InlineSpanSource, LayoutBox, ResolvedBorder};

use build::build;

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
        initial.font_family.unwrap_or_default(),
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
