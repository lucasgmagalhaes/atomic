//! `Dimensions`/`ResolvedBorder`/`InlineSpanSource`/`LayoutBox` — split out
//! from `tree/mod.rs`.

use dom::NodeId;

use crate::style::{Color, ComputedStyle, FontFamily};
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
    pub font_family: FontFamily,
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
    /// [`super::apply_image_sizes`] — box tree construction itself never
    /// fetches anything over the network). `None` for every other box, and
    /// for an `<img>` with no `src`, a failed fetch, or undecodable bytes -
    /// those all render as an empty box, same as this engine already
    /// treats an unknown/unstyled element.
    pub image: Option<std::rc::Rc<image_decode::DecodedImage>>,
    /// Real per-element scroll offset (`ROADMAP.md` item 22), read from
    /// `dom::Dom::element_scroll_offset` at box-tree construction time -
    /// `(0.0, 0.0)` for a text/synthetic-inline-run box (neither is ever a
    /// real scroll container) and for any element never scrolled. Only
    /// meaningful when this box's own `style.overflow` isn't `Visible`
    /// (real CSS: only a non-`visible` overflow value makes an element a
    /// scroll container) - `render::display_list` is the one consumer.
    pub scroll_offset: (f64, f64),
}
