//! Token-parsing helpers for individual/shorthand properties — called
//! from `apply_declaration.rs`'s per-property match arms.

use css::Token;

use super::computed_style::ComputedStyle;
use super::types::{BorderStyle, BoxShadow, Color, EdgeSizes, Length};

pub(super) fn parse_length(token: &Token) -> Option<Length> {
    match token {
        Token::Dimension(n, unit) if unit == "px" => Some(Length::Px(*n)),
        Token::Percentage(n) => Some(Length::Percent(*n)),
        Token::Number(n) if *n == 0.0 => Some(Length::Px(0.0)),
        Token::Ident(s) if s == "auto" => Some(Length::Auto),
        _ => None,
    }
}

pub(super) fn parse_edge_shorthand(tokens: &[Token]) -> Option<EdgeSizes> {
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
pub(super) fn apply_border_shorthand(style: &mut ComputedStyle, tokens: &[Token]) {
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
pub(super) fn apply_box_shadow(style: &mut ComputedStyle, tokens: &[Token]) {
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

/// Real, but 2D-translate-only `transform` — see
/// `ComputedStyle::transform`'s own doc for the full scope. `none` resets
/// to `(0.0, 0.0)`; otherwise scans the token stream for
/// `translate`/`translateX`/`translateY` function calls (one or more,
/// space-separated real transform functions compose in the source
/// order — this narrower pass just sums every translate offset it
/// finds, ignoring any other function name's args entirely rather than
/// trying to interpret them). A length argument's sign uses the same
/// `Delim('-')`-then-number two-token lookahead `"z-index"`'s own
/// parsing already documents (the lexer never folds a unary minus into
/// a `Number`/`Dimension` token).
pub(super) fn apply_transform(style: &mut ComputedStyle, tokens: &[Token]) {
    if let Some(Token::Ident(v)) = tokens.first() {
        if v == "none" {
            style.transform = (0.0, 0.0);
            return;
        }
    }
    /// Reads one length value (optionally signed) starting at `tokens[*i]`,
    /// advancing `*i` past it. `None` if `tokens[*i]` isn't a length at all
    /// (e.g. it's already `,`/`)`).
    fn read_length(tokens: &[Token], i: &mut usize) -> Option<f64> {
        match tokens.get(*i) {
            Some(Token::Delim('-')) => {
                let n = match tokens.get(*i + 1) {
                    Some(Token::Dimension(n, _)) => *n,
                    Some(Token::Number(n)) => *n,
                    _ => return None,
                };
                *i += 2;
                Some(-n)
            }
            Some(Token::Dimension(n, _)) => {
                *i += 1;
                Some(*n)
            }
            Some(Token::Number(n)) => {
                *i += 1;
                Some(*n)
            }
            _ => None,
        }
    }

    let mut dx = 0.0;
    let mut dy = 0.0;
    let mut i = 0;
    while i < tokens.len() {
        let Some(Token::Ident(name)) = tokens.get(i) else {
            i += 1;
            continue;
        };
        let name = name.to_ascii_lowercase();
        i += 1;
        if tokens.get(i) != Some(&Token::LParen) {
            continue;
        }
        i += 1;
        let mut args = Vec::new();
        while tokens.get(i) != Some(&Token::RParen) && i < tokens.len() {
            if let Some(n) = read_length(tokens, &mut i) {
                args.push(n);
            } else {
                i += 1;
            }
        }
        i += 1; // past RParen (or end of input if malformed - harmless)
        match name.as_str() {
            "translate" => {
                dx += args.first().copied().unwrap_or(0.0);
                dy += args.get(1).copied().unwrap_or(0.0);
            }
            "translatex" => dx += args.first().copied().unwrap_or(0.0),
            "translatey" => dy += args.first().copied().unwrap_or(0.0),
            _ => {}
        }
    }
    style.transform = (dx, dy);
}

/// Real `border-radius: <length>` — single uniform value only (see
/// [`ComputedStyle::border_radius`]'s own doc for the per-corner scope
/// cut). `0`/`0px` explicitly clears a previously cascaded radius back to
/// square corners, same reset behavior every other length property here
/// already has.
pub(super) fn apply_border_radius(style: &mut ComputedStyle, tokens: &[Token]) {
    if let Some(Length::Px(px)) = tokens.first().and_then(parse_length) {
        style.border_radius = px;
    }
}
