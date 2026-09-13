//! Applies one cascaded `Declaration` onto a `ComputedStyle` — one match
//! arm per property this crate models. Called once per declaration, in
//! cascade order, by `super::resolve_style`.

use css::{Declaration, Token};

use super::computed_style::ComputedStyle;
use super::property_parsers::{
    apply_border_radius, apply_border_shorthand, apply_box_shadow, apply_transform,
    parse_edge_shorthand, parse_length,
};
use super::types::{
    AlignItems, BorderStyle, Clear, Color, Display, FlexDirection, Float, JustifyContent, Overflow,
    Position,
};

pub(super) fn apply_declaration(style: &mut ComputedStyle, decl: &Declaration) {
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
        "transform" => apply_transform(style, &decl.value),
        _ => {}
    }
}
