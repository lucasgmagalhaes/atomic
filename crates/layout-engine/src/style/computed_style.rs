//! `ComputedStyle` — the typed struct every cascaded rule set resolves
//! into (see `super::resolve_style`). Field docs describe each
//! property's exact scope cut versus the full spec.

use super::types::{
    AlignItems, BorderStyle, BoxShadow, Clear, Color, Display, EdgeSizes, FlexDirection, Float,
    GridTracks, JustifyContent, Length, LinearGradient, ListStylePosition, ListStyleType, Overflow,
    Position,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComputedStyle {
    pub display: Display,
    pub width: Length,
    pub height: Length,
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,
    pub position: Position,
    /// Only meaningful when `position` is `Relative` (a visual-only shift,
    /// applied post-flow-placement - see `layout::layout_block`) or
    /// `Absolute` (drives the box's actual position - see
    /// `layout::layout_children`'s own doc). `right`/`bottom` are real but
    /// narrower-scoped than the spec: only consulted when the
    /// corresponding `left`/`top` is `Auto` (no over-constrained-box
    /// resolution), and always resolved as plain pixel offsets - a
    /// percentage `top`/`bottom` falls back to `0.0` since resolving it
    /// correctly needs the containing block's own definite height, which
    /// this crate's single top-down layout pass doesn't have on hand.
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
    /// `Auto`/`Percent` are never produced by real `border-width` CSS (the
    /// property only accepts a length or a `thin`/`medium`/`thick`
    /// keyword, neither of which this crate parses) - reuses `EdgeSizes`/
    /// `Length` purely for its existing shorthand-parsing machinery
    /// (`parse_edge_shorthand`), not because those variants are
    /// meaningful here. Real box-model space when `border_style` isn't
    /// `None` — see `layout::layout_block`.
    pub border_width: EdgeSizes,
    pub border_style: BorderStyle,
    /// Real content clipping - see [`Overflow`]'s own doc for the
    /// `hidden`/`auto`/`scroll` scope cut.
    pub overflow: Overflow,
    /// See [`Float`]'s own doc for the real, narrower-than-spec scope.
    pub float: Float,
    /// See [`Clear`]'s own doc.
    pub clear: Clear,
    /// `None` is the initial value (`box-shadow: none`) — see
    /// [`BoxShadow`]'s own doc for the real scope cut.
    pub box_shadow: Option<BoxShadow>,
    /// One color for all four sides - real per-side colors
    /// (`border-top-color`, ...) aren't modeled. Initial value is black,
    /// not the spec's `currentColor` (which would need reading back
    /// `color` at border-paint time, not just at cascade time) - a
    /// documented simplification, same shape as `font-size`'s own
    /// "px only, no keyword sizes" scope cut.
    pub border_color: Color,
    /// One radius for all four corners - real per-corner radii
    /// (`border-top-left-radius`, ...) and the two-value-per-corner
    /// elliptical form aren't modeled, same "one shorthand value only"
    /// scope cut `border_width`'s own doc already takes. `px` only,
    /// matching `font_size`. `0.0` (square corners) is the initial value.
    pub border_radius: f64,
    /// Only meaningful when this box's own `display` is `Flex` - controls
    /// how *its* children are arranged.
    pub flex_direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    /// The following three are only meaningful when this box is itself a
    /// flex *item* (i.e. its parent is `display: flex`).
    pub flex_grow: f64,
    pub flex_shrink: f64,
    pub flex_basis: Length,
    /// Only meaningful when this box's own `display` is `Grid` — the
    /// explicit column/row track list (`ROADMAP.md` item 34). Empty
    /// means no explicit tracks: `crate::grid` falls back to a single
    /// full-width column for `grid_template_columns`, and content-sized
    /// (auto) rows for `grid_template_rows` — see that module's own doc.
    pub grid_template_columns: GridTracks,
    pub grid_template_rows: GridTracks,
    pub background_color: Color,
    /// `None` (the common case — a flat `background-color`) paints exactly
    /// as before this field existed. See [`LinearGradient`]'s own doc for
    /// the real scope cut. Not inherited (matches real `background`).
    pub background_image: Option<LinearGradient>,
    /// The two *inherited* properties this crate models (`resolve_style`
    /// takes the parent's resolved values as the starting point instead of
    /// the fixed initial values, per CSS inheritance rules) - every other
    /// property resolves independently of the parent.
    /// `px` only, no `em`/`rem`/keyword sizes.
    pub font_size: f64,
    /// Text color - initial value is black, matching the real spec.
    pub color: Color,
    /// Real, but per-primitive rather than per-group: real CSS renders a
    /// box's whole subtree to an offscreen layer once, then blends that
    /// *one* flattened result at `opacity` - this crate has no offscreen
    /// compositing pass, so `render::display_list` instead multiplies
    /// `opacity` into every individual paint primitive's own alpha as it
    /// walks the tree (an ancestor's `opacity` compounds into its
    /// descendants' effective opacity, same real "nested opacity
    /// multiplies" semantics the spec has). The one real, visible
    /// divergence: two overlapping siblings inside the same opacity group
    /// blend against *each other* at the reduced alpha (a visible seam
    /// where they overlap) instead of being invisible to each other until
    /// the whole flattened group is blended onto what's behind it. `1.0`
    /// (opaque) is the initial value; values are clamped to `[0.0, 1.0]`
    /// at parse time, matching the spec's own out-of-range clamping.
    pub opacity: f64,
    /// `None` is `z-index: auto`, the initial value — a box with `auto`
    /// never itself establishes a stacking context (only `position !=
    /// Static` *and* an explicit integer together do — see
    /// `render::display_list`'s paint-order sort, which is the only
    /// consumer of this field; layout/sizing never reads it). `Some(n)`
    /// for any parsed integer, negative included.
    pub z_index: Option<i32>,
    /// Real `transform`, scoped to 2D translation only — `(translate_x,
    /// translate_y)` in px, `(0.0, 0.0)` (the initial value) meaning
    /// `none`/no transform. Parses `translate(x[, y])`, `translateX(x)`,
    /// `translateY(y)` (multiple functions in one value compose
    /// additively, matching real translate-composition semantics without
    /// a real matrix); `rotate`/`scale`/`skew`/`matrix` are recognized
    /// (skipped past, so the parser stays synchronized) but contribute no
    /// offset — this crate's paint primitives are axis-aligned rects with
    /// no rotation/scale support (see `render::display_list`, the only
    /// consumer), same "real but narrower" scope cut `border_radius`'s
    /// own doc already documents for this codebase. A transform never
    /// affects layout — siblings/ancestors are positioned as if untransformed,
    /// exactly like `opacity` — only where this box (and its subtree) paints.
    pub transform: (f64, f64),
    /// `None` means unset — `tree::build` falls back to a tag-based default
    /// (`decimal` for an `<ol>`'s items, `disc` for an `<ul>`'s) when
    /// generating a `<li>`'s marker, since this crate has no user-agent
    /// stylesheet to express that default as an initial value the normal
    /// way. See [`ListStyleType`]'s own doc for the real scope cut.
    pub list_style_type: Option<ListStyleType>,
    /// See [`ListStylePosition`]'s own doc — real initial value
    /// (`outside`), but has no distinct effect in this crate.
    pub list_style_position: ListStylePosition,
}

impl ComputedStyle {
    /// CSS initial values for the properties this crate models.
    pub fn initial() -> Self {
        ComputedStyle {
            display: Display::Block,
            width: Length::Auto,
            height: Length::Auto,
            margin: EdgeSizes::zero(),
            padding: EdgeSizes::zero(),
            position: Position::Static,
            top: Length::Auto,
            right: Length::Auto,
            bottom: Length::Auto,
            left: Length::Auto,
            border_width: EdgeSizes::zero(),
            border_style: BorderStyle::None,
            overflow: Overflow::Visible,
            float: Float::None,
            clear: Clear::None,
            box_shadow: None,
            border_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            border_radius: 0.0,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Stretch,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            grid_template_columns: GridTracks::EMPTY,
            grid_template_rows: GridTracks::EMPTY,
            background_color: Color::TRANSPARENT,
            background_image: None,
            font_size: 16.0,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            opacity: 1.0,
            z_index: None,
            transform: (0.0, 0.0),
            list_style_type: None,
            list_style_position: ListStylePosition::Outside,
        }
    }
}
