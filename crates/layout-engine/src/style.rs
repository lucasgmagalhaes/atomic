//! Cascaded declarations → typed computed style. Scoped property set:
//! `display` (block/inline/flex/none), `width`/`height`, `margin`/
//! `padding` (shorthands + longhands), and the flex properties
//! `flex-direction`/`justify-content`/`align-items`/`flex-grow`/
//! `flex-shrink`/`flex-basis`, and `background-color` (also accepted as
//! `background`, but only the solid-color form — no gradients/images).
//! No inheritance yet (every property resolves independently of the
//! parent's computed style) and no `flex` shorthand (only the longhands)
//! or positioning properties.
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
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };

    fn named(name: &str) -> Option<Color> {
        Some(match name {
            "transparent" => Color::TRANSPARENT,
            "black" => Color { r: 0, g: 0, b: 0, a: 255 },
            "white" => Color { r: 255, g: 255, b: 255, a: 255 },
            "red" => Color { r: 255, g: 0, b: 0, a: 255 },
            "green" => Color { r: 0, g: 128, b: 0, a: 255 },
            "blue" => Color { r: 0, g: 0, b: 255, a: 255 },
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
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Stretch,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            background_color: Color::TRANSPARENT,
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
        _ => {}
    }
}

/// Applies `matched` (already cascade-ordered lowest to highest priority,
/// see `css::matching_declarations`) on top of the initial values.
pub fn resolve_style(matched: &[MatchedDeclarations]) -> ComputedStyle {
    let mut style = ComputedStyle::initial();
    for m in matched {
        for decl in m.declarations {
            apply_declaration(&mut style, decl);
        }
    }
    style
}
