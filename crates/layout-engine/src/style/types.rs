//! Simple enum/struct property types — no CSS parsing logic here (see
//! `property_parsers.rs`), just the typed shapes `ComputedStyle`
//! (`computed_style.rs`) is built from.

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

    pub(super) fn named(name: &str) -> Option<Color> {
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
    pub(super) fn from_hex(hex: &str) -> Option<Color> {
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
    pub(super) fn zero() -> Self {
        EdgeSizes {
            top: Length::Px(0.0),
            right: Length::Px(0.0),
            bottom: Length::Px(0.0),
            left: Length::Px(0.0),
        }
    }
}
