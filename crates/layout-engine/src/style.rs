//! Cascaded declarations → typed computed style. Scoped property set:
//! `display` (block/inline/flex/none), `width`/`height`, `margin`/
//! `padding` (shorthands + longhands), `position` (`static`/`relative`/
//! `absolute`) with `top`/`right`/`bottom`/`left`, `border-width`/
//! `border-style`/`border-color` (plus the `border` shorthand — solid
//! only, one color for all four sides, real box-model growth not just a
//! paint decoration), and the flex properties `flex-direction`/
//! `justify-content`/`align-items`/`flex-grow`/`flex-shrink`/
//! `flex-basis`, `background-color` (also accepted as `background`, but
//! only the solid-color form — no gradients/images), `font-size` (px
//! only), and `color`. `font-size` and `color` are the only properties
//! this crate inherits — every other property resolves independently of
//! the parent's computed style. No `flex`/`border` per-side-longhand
//! shorthand beyond what's listed above.
use css::{Declaration, MatchedDeclarations, Token};

/// Straight (non-premultiplied) sRGB + alpha, each channel `0..=255`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    fn named(name: &str) -> Option<Color> {
        Some(match name {
            "transparent" => Color::TRANSPARENT,
            "black" => Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            "white" => Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            "red" => Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            "green" => Color {
                r: 0,
                g: 128,
                b: 0,
                a: 255,
            },
            "blue" => Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255,
            },
            _ => return None,
        })
    }

    /// `#rgb` or `#rrggbb` (no alpha channel form yet - `#rgba`/`#rrggbbaa`).
    fn from_hex(hex: &str) -> Option<Color> {
        let expand = |c: char| c.to_digit(16).map(|d| (d as u8) * 17); // 0xF -> 0xFF
        match hex.len() {
            3 => {
                let mut chars = hex.chars();
                Some(Color {
                    r: expand(chars.next()?)?,
                    g: expand(chars.next()?)?,
                    b: expand(chars.next()?)?,
                    a: 255,
                })
            }
            6 => {
                let byte = |s: &str| u8::from_str_radix(s, 16).ok();
                Some(Color {
                    r: byte(&hex[0..2])?,
                    g: byte(&hex[2..4])?,
                    b: byte(&hex[4..6])?,
                    a: 255,
                })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f64),
    Percent(f64),
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    Flex,
    None,
}

/// `Fixed`/`Sticky` aren't modeled — see `layout::layout_children`'s own
/// doc on the real, narrower-than-spec scope `Absolute` gets here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
}

/// `dashed`/`dotted`/`double`/`groove`/... aren't modeled — a border only
/// ever paints as a solid rectangle (see `render::build_display_list`),
/// matching this crate's existing "flat solid quads only" painting model
/// (no stroke/dash patterns anywhere, same scope `background-color`
/// already has).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    None,
    Solid,
}

/// `hidden`/`auto`/`scroll` all resolve to `Hidden` here - real CSS clips
/// content against the box's own border box for all three (only
/// `visible` doesn't), and that clipping is what this crate models (see
/// `render::build_display_list`/`build_image_list`/`build_glyph_list`).
/// What's *not* modeled is the difference between them: no scrollbar is
/// ever drawn, and there's no independent inner scroll offset for an
/// `auto`/`scroll` container (only the whole-viewport scroll
/// `profile-worker`'s `SCROLL` command already has) - a real, narrower
/// scope cut of the same gap `mockup/rendering-engine-gaps.md`'s Scroll
/// section already documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Hidden,
}

/// Real, but narrower than the spec — see `layout::layout_children`'s own
/// doc for exactly what's modeled: a floated box is removed from normal
/// block stacking and placed flush to its containing block's left/right
/// edge, not overlapping an earlier same-side float, but nothing wraps
/// inline content around it (no line-box narrowing) and a `width: auto`
/// float stretches to fill available width same as a normal block rather
/// than shrink-to-fit sizing to its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Float {
    None,
    Left,
    Right,
}

/// See [`Float`]'s own doc — `clear` pushes a box below the bottom of
/// whichever side(s) it names, tracked per containing block the same way
/// `layout::layout_children` tracks float placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clear {
    None,
    Left,
    Right,
    Both,
}

/// A real, but flat, `box-shadow` — no `blur-radius` (this crate's
/// "solid rects only" paint pipeline has no blur/gaussian geometry, same
/// scope cut `border-radius` already has for rounded corners), and only
/// one shadow even though real CSS accepts a comma-separated list (same
/// "one value, not a list" scope `border-color` already has for per-side
/// colors). `spread` grows/shrinks the shadow rect on every side, same
/// real effect a spread radius has minus the corner rounding a nonzero
/// `border-radius` would also apply to it. `inset` shadows aren't
/// recognized (the `inset` keyword just fails `Color::named` silently
/// and is ignored, same as any other unrecognized ident in this
/// declaration) - every shadow this crate paints is a real drop shadow.
/// `color` is required in the source (real CSS lets it default to
/// `currentColor` when omitted; this crate doesn't track declaration
/// order finely enough within one rule to resolve that reliably, so an
/// omitted color just means no shadow is set at all, same as `none`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f64,
    pub offset_y: f64,
    pub spread: f64,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Start,
    End,
    Center,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSizes {
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
}

impl EdgeSizes {
    fn zero() -> Self {
        EdgeSizes {
            top: Length::Px(0.0),
            right: Length::Px(0.0),
            bottom: Length::Px(0.0),
            left: Length::Px(0.0),
        }
    }
}

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
    pub background_color: Color,
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
            background_color: Color::TRANSPARENT,
            font_size: 16.0,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            opacity: 1.0,
            z_index: None,
        }
    }
}

fn parse_length(token: &Token) -> Option<Length> {
    match token {
        Token::Dimension(n, unit) if unit == "px" => Some(Length::Px(*n)),
        Token::Percentage(n) => Some(Length::Percent(*n)),
        Token::Number(n) if *n == 0.0 => Some(Length::Px(0.0)),
        Token::Ident(s) if s == "auto" => Some(Length::Auto),
        _ => None,
    }
}

fn parse_edge_shorthand(tokens: &[Token]) -> Option<EdgeSizes> {
    let lengths: Vec<Length> = tokens.iter().filter_map(parse_length).collect();
    // Every token must have parsed - a shorthand with one bad value is
    // invalid as a whole, per CSS error handling (falls back to whatever
    // was cascaded in from a lower-priority rule, or the initial value).
    if lengths.len() != tokens.len() || lengths.is_empty() {
        return None;
    }
    Some(match lengths.as_slice() {
        [all] => EdgeSizes {
            top: *all,
            right: *all,
            bottom: *all,
            left: *all,
        },
        [vertical, horizontal] => EdgeSizes {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        },
        [top, horizontal, bottom] => EdgeSizes {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        },
        [top, right, bottom, left, ..] => EdgeSizes {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        },
        [] => unreachable!("checked non-empty above"),
    })
}

/// Real `border: <width> <style> <color>` shorthand — real CSS accepts
/// the three components in any order, and each is individually optional
/// (an omitted component leaves whatever it already cascaded to
/// unchanged, matching the other shorthands in this file - `background`/
/// `margin` don't reset unspecified sub-properties to their initial
/// value either). Sets all four sides identically - no way to express
/// per-side values through this one property, same as real CSS's own
/// `border` shorthand.
fn apply_border_shorthand(style: &mut ComputedStyle, tokens: &[Token]) {
    let mut width = None;
    let mut border_style = None;
    let mut color = None;
    for token in tokens {
        if let Some(l) = parse_length(token) {
            width = Some(l);
            continue;
        }
        match token {
            Token::Ident(v) if v == "none" => border_style = Some(BorderStyle::None),
            Token::Ident(v) if v == "solid" => border_style = Some(BorderStyle::Solid),
            Token::Ident(name) => color = color.or(Color::named(name)),
            Token::Hash(hex) => color = color.or(Color::from_hex(hex)),
            _ => {}
        }
    }
    if let Some(w) = width {
        style.border_width = EdgeSizes {
            top: w,
            right: w,
            bottom: w,
            left: w,
        };
    }
    if let Some(s) = border_style {
        style.border_style = s;
    }
    if let Some(c) = color {
        style.border_color = c;
    }
}

/// Real `box-shadow: <offset-x> <offset-y> [<blur-radius>] [<spread-radius>] <color>`
/// - `blur-radius`, if present, is parsed (so it doesn't get mistaken for
/// `spread-radius`) but not stored anywhere (see [`BoxShadow`]'s own doc
/// on why blur isn't modeled). `none` clears any previously cascaded
/// shadow, same as any other property's keyword reset.
fn apply_box_shadow(style: &mut ComputedStyle, tokens: &[Token]) {
    if let Some(Token::Ident(v)) = tokens.first() {
        if v == "none" {
            style.box_shadow = None;
            return;
        }
    }
    let mut lengths: Vec<f64> = Vec::new();
    let mut color = None;
    for token in tokens {
        match token {
            Token::Dimension(n, unit) if unit == "px" => lengths.push(*n),
            Token::Number(n) if *n == 0.0 => lengths.push(0.0),
            Token::Ident(name) => color = color.or(Color::named(name)),
            Token::Hash(hex) => color = color.or(Color::from_hex(hex)),
            _ => {}
        }
    }
    // `lengths[2]` (blur-radius), when present, is deliberately skipped -
    // only offsets and spread feed the flat rect this crate paints.
    if let (Some(&offset_x), Some(&offset_y), Some(color)) =
        (lengths.first(), lengths.get(1), color)
    {
        let spread = lengths.get(3).copied().unwrap_or(0.0);
        style.box_shadow = Some(BoxShadow {
            offset_x,
            offset_y,
            spread,
            color,
        });
    }
}

/// Real `border-radius: <length>` — single uniform value only (see
/// [`ComputedStyle::border_radius`]'s own doc for the per-corner scope
/// cut). `0`/`0px` explicitly clears a previously cascaded radius back to
/// square corners, same reset behavior every other length property here
/// already has.
fn apply_border_radius(style: &mut ComputedStyle, tokens: &[Token]) {
    if let Some(Length::Px(px)) = tokens.first().and_then(parse_length) {
        style.border_radius = px;
    }
}

fn apply_declaration(style: &mut ComputedStyle, decl: &Declaration) {
    match decl.name.as_str() {
        "display" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                style.display = match v.as_str() {
                    "block" => Display::Block,
                    "inline" => Display::Inline,
                    "flex" => Display::Flex,
                    "none" => Display::None,
                    _ => return,
                };
            }
        }
        "flex-direction" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                style.flex_direction = match v.as_str() {
                    "row" => FlexDirection::Row,
                    "column" => FlexDirection::Column,
                    _ => return,
                };
            }
        }
        "justify-content" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                style.justify_content = match v.as_str() {
                    "flex-start" => JustifyContent::Start,
                    "flex-end" => JustifyContent::End,
                    "center" => JustifyContent::Center,
                    "space-between" => JustifyContent::SpaceBetween,
                    "space-around" => JustifyContent::SpaceAround,
                    _ => return,
                };
            }
        }
        "align-items" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                style.align_items = match v.as_str() {
                    "flex-start" => AlignItems::Start,
                    "flex-end" => AlignItems::End,
                    "center" => AlignItems::Center,
                    "stretch" => AlignItems::Stretch,
                    _ => return,
                };
            }
        }
        "flex-grow" => {
            if let Some(Token::Number(n)) = decl.value.first() {
                style.flex_grow = *n;
            }
        }
        "flex-shrink" => {
            if let Some(Token::Number(n)) = decl.value.first() {
                style.flex_shrink = *n;
            }
        }
        "flex-basis" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.flex_basis = l;
            }
        }
        "background-color" | "background" => {
            let color = match decl.value.first() {
                Some(Token::Ident(name)) => Color::named(name),
                Some(Token::Hash(hex)) => Color::from_hex(hex),
                _ => None,
            };
            if let Some(c) = color {
                style.background_color = c;
            }
        }
        "font-size" => {
            // Only absolute px, matching parse_length's own scope - no
            // em/rem (which would need the inherited value mid-parse) or
            // keyword sizes (medium/large/...).
            if let Some(Token::Dimension(n, unit)) = decl.value.first() {
                if unit == "px" {
                    style.font_size = *n;
                }
            }
        }
        "color" => {
            let color = match decl.value.first() {
                Some(Token::Ident(name)) => Color::named(name),
                Some(Token::Hash(hex)) => Color::from_hex(hex),
                _ => None,
            };
            if let Some(c) = color {
                style.color = c;
            }
        }
        "width" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.width = l;
            }
        }
        "height" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.height = l;
            }
        }
        "margin" => {
            if let Some(e) = parse_edge_shorthand(&decl.value) {
                style.margin = e;
            }
        }
        "padding" => {
            if let Some(e) = parse_edge_shorthand(&decl.value) {
                style.padding = e;
            }
        }
        "margin-top" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.margin.top = l;
            }
        }
        "margin-right" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.margin.right = l;
            }
        }
        "margin-bottom" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.margin.bottom = l;
            }
        }
        "margin-left" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.margin.left = l;
            }
        }
        "padding-top" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.padding.top = l;
            }
        }
        "padding-right" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.padding.right = l;
            }
        }
        "padding-bottom" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.padding.bottom = l;
            }
        }
        "padding-left" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.padding.left = l;
            }
        }
        "position" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                style.position = match v.as_str() {
                    "static" => Position::Static,
                    "relative" => Position::Relative,
                    "absolute" => Position::Absolute,
                    _ => return,
                };
            }
        }
        "top" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.top = l;
            }
        }
        "right" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.right = l;
            }
        }
        "bottom" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.bottom = l;
            }
        }
        "left" => {
            if let Some(l) = decl.value.first().and_then(parse_length) {
                style.left = l;
            }
        }
        "border-width" => {
            if let Some(e) = parse_edge_shorthand(&decl.value) {
                style.border_width = e;
            }
        }
        "border-style" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                style.border_style = match v.as_str() {
                    "none" => BorderStyle::None,
                    "solid" => BorderStyle::Solid,
                    _ => return,
                };
            }
        }
        "border-color" => {
            let color = match decl.value.first() {
                Some(Token::Ident(name)) => Color::named(name),
                Some(Token::Hash(hex)) => Color::from_hex(hex),
                _ => None,
            };
            if let Some(c) = color {
                style.border_color = c;
            }
        }
        "border" => apply_border_shorthand(style, &decl.value),
        "overflow" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                style.overflow = match v.as_str() {
                    "visible" => Overflow::Visible,
                    "hidden" | "auto" | "scroll" => Overflow::Hidden,
                    _ => return,
                };
            }
        }
        "float" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                style.float = match v.as_str() {
                    "none" => Float::None,
                    "left" => Float::Left,
                    "right" => Float::Right,
                    _ => return,
                };
            }
        }
        "clear" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                style.clear = match v.as_str() {
                    "none" => Clear::None,
                    "left" => Clear::Left,
                    "right" => Clear::Right,
                    "both" => Clear::Both,
                    _ => return,
                };
            }
        }
        "box-shadow" => apply_box_shadow(style, &decl.value),
        "border-radius" => apply_border_radius(style, &decl.value),
        "opacity" => {
            let value = match decl.value.first() {
                Some(Token::Number(n)) => Some(*n),
                Some(Token::Percentage(p)) => Some(p / 100.0),
                _ => None,
            };
            if let Some(v) = value {
                style.opacity = v.clamp(0.0, 1.0);
            }
        }
        "z-index" => {
            style.z_index = match decl.value.first() {
                Some(Token::Ident(v)) if v == "auto" => None,
                // The lexer never folds a unary minus into `Number` (see
                // its own `starts_ident` doc) - `-1` tokenizes as
                // `Delim('-')` then `Number(1.0)`, so a negative z-index
                // (a real, common value - see `render::display_list`'s
                // paint-order sort) needs this two-token lookahead, unlike
                // every other numeric property here.
                Some(Token::Delim('-')) => match decl.value.get(1) {
                    Some(Token::Number(n)) => Some(-(*n as i32)),
                    _ => style.z_index,
                },
                Some(Token::Number(n)) => Some(*n as i32),
                _ => style.z_index,
            };
        }
        _ => {}
    }
}

/// Applies `matched` (already cascade-ordered lowest to highest priority,
/// see `css::matching_declarations`) on top of the initial values.
/// `parent_font_size` seeds the one inherited property this crate models
/// — pass `ComputedStyle::initial().font_size` for the document root,
/// which has no parent to inherit from.
pub fn resolve_style(
    matched: &[MatchedDeclarations],
    parent_font_size: f64,
    parent_color: Color,
) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    style.font_size = parent_font_size;
    style.color = parent_color;
    for m in matched {
        for decl in m.declarations {
            apply_declaration(&mut style, decl);
        }
    }
    style
}
