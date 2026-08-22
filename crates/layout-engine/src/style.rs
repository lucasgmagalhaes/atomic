//! Cascaded declarations → typed computed style. Scoped property set:
//! `display` (block/inline/none), `width`/`height`, and the `margin`/
//! `padding` shorthands + longhands. No inheritance yet (every property
//! resolves independently of the parent's computed style) and no
//! `flex`/positioning properties — those land with the flex layout pass.
use css::{Declaration, MatchedDeclarations, Token};

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
    None,
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
                    "none" => Display::None,
                    _ => return,
                };
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
