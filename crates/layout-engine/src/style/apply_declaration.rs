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
    AlignItems, BorderStyle, Clear, Color, Display, FlexDirection, Float, FontFamily,
    GenericFontFamily, GridTrackSize, GridTracks, JustifyContent, LinearGradient,
    ListStylePosition, ListStyleType, Overflow, Position,
};

/// `None` for anything but the 5 real CSS generic keywords this crate
/// models - see [`GenericFontFamily`]'s own doc.
fn parse_generic_font_family(v: &str) -> Option<GenericFontFamily> {
    match v {
        "serif" => Some(GenericFontFamily::Serif),
        "sans-serif" => Some(GenericFontFamily::SansSerif),
        "monospace" => Some(GenericFontFamily::Monospace),
        "cursive" => Some(GenericFontFamily::Cursive),
        "fantasy" => Some(GenericFontFamily::Fantasy),
        _ => None,
    }
}

/// Parses a `font-family` value - real CSS's own comma-separated fallback
/// list, scoped down to the one real-world-common pattern (see
/// [`FontFamily`]'s own doc): the first entry that isn't a generic keyword
/// becomes the specific name, and the *last* generic keyword anywhere in
/// the list becomes the fallback (real pages almost always put exactly one
/// generic keyword last, e.g. `"Helvetica Neue", Arial, sans-serif`, so
/// this reads the same as the real cascade would for that common shape).
/// `None` only when the value has neither a usable name nor a recognized
/// generic keyword (e.g. it's empty, or every entry is an unrecognized
/// custom `@font-face` name with no generic fallback at all).
fn parse_font_family(value: &[Token]) -> Option<FontFamily> {
    let mut name: Option<String> = None;
    let mut generic: Option<GenericFontFamily> = None;
    for segment in value.split(|t| *t == Token::Comma) {
        let Some(first) = segment.first() else {
            continue;
        };
        let entry = match first {
            Token::Ident(v) => v.as_str(),
            Token::String(v) => v.as_str(),
            _ => continue,
        };
        if let Some(g) = parse_generic_font_family(entry) {
            generic = Some(g);
        } else if name.is_none() {
            name = Some(entry.to_string());
        }
    }
    if name.is_none() && generic.is_none() {
        return None;
    }
    let generic = generic.unwrap_or(GenericFontFamily::SansSerif);
    match name {
        Some(n) => Some(FontFamily::named(&n, generic)),
        None => Some(FontFamily::generic(generic)),
    }
}

/// Parses one `linear-gradient(...)` value - real, but 2-stop only, see
/// [`LinearGradient`]'s own doc. `tokens` is everything strictly between
/// the function's own `LParen`/`RParen` (the caller already found and
/// stripped those). The first comma-separated segment is an optional
/// direction (`<angle>deg`, or a `to <side>[ <side>]` keyword pair,
/// defaulting to `180.0` - real CSS's own default direction, `to bottom`
/// - when the first segment parses as neither and is instead the first
/// color); every segment after that is a color, of which only the first
/// and last are kept (real CSS's own multi-stop list isn't modeled, same
/// "one value" scope cut `BoxShadow` already takes elsewhere in this
/// file).
fn parse_linear_gradient(tokens: &[Token]) -> Option<LinearGradient> {
    let segments: Vec<&[Token]> = tokens.split(|t| *t == Token::Comma).collect();
    if segments.len() < 2 {
        return None;
    }

    let parse_color = |seg: &[Token]| -> Option<Color> {
        match seg.first()? {
            Token::Ident(name) => Color::named(name),
            Token::Hash(hex) => Color::from_hex(hex),
            _ => None,
        }
    };

    let angle_from_direction = |seg: &[Token]| -> Option<f64> {
        if let [Token::Dimension(n, unit)] = seg {
            if unit.eq_ignore_ascii_case("deg") {
                return Some(*n);
            }
        }
        let idents: Vec<&str> = seg
            .iter()
            .filter_map(|t| match t {
                Token::Ident(v) => Some(v.as_str()),
                _ => None,
            })
            .collect();
        if idents.first() != Some(&"to") {
            return None;
        }
        let sides: Vec<&str> = idents[1..].to_vec();
        match sides.as_slice() {
            ["top"] => Some(0.0),
            ["right"] => Some(90.0),
            ["bottom"] => Some(180.0),
            ["left"] => Some(270.0),
            ["top", "right"] | ["right", "top"] => Some(45.0),
            ["bottom", "right"] | ["right", "bottom"] => Some(135.0),
            ["bottom", "left"] | ["left", "bottom"] => Some(225.0),
            ["top", "left"] | ["left", "top"] => Some(315.0),
            _ => None,
        }
    };

    let (angle_deg, color_segments) = match angle_from_direction(segments[0]) {
        Some(angle) => (angle, &segments[1..]),
        None => (180.0, &segments[..]),
    };
    if color_segments.len() < 2 {
        return None;
    }
    let from = parse_color(color_segments[0])?;
    let to = parse_color(color_segments[color_segments.len() - 1])?;
    Some(LinearGradient {
        angle_deg,
        from,
        to,
    })
}

/// `true` when `value` opens with `linear-gradient(...)` - shared by the
/// `"background"`/`"background-image"` arms below.
fn is_linear_gradient_call(value: &[Token]) -> bool {
    matches!(value.first(), Some(Token::Ident(name)) if name == "linear-gradient")
        && matches!(value.get(1), Some(Token::LParen))
}

/// The inner tokens of a `linear-gradient(...)` call already confirmed by
/// [`is_linear_gradient_call`] - everything between the matching parens.
/// This crate's lexer never nests parens inside a gradient argument (no
/// nested function calls are parsed), so a flat scan for the first
/// `RParen` is exact, not just a heuristic.
fn linear_gradient_inner(value: &[Token]) -> &[Token] {
    let close = value
        .iter()
        .position(|t| *t == Token::RParen)
        .unwrap_or(value.len());
    &value[2..close]
}

/// Shared by `"list-style-type"` and the `"list-style"` shorthand.
fn parse_list_style_type(v: &str) -> Option<ListStyleType> {
    match v {
        "disc" => Some(ListStyleType::Disc),
        "circle" => Some(ListStyleType::Circle),
        "square" => Some(ListStyleType::Square),
        "decimal" => Some(ListStyleType::Decimal),
        "none" => Some(ListStyleType::None),
        _ => None,
    }
}

/// Shared by `"list-style-position"` and the `"list-style"` shorthand.
fn parse_list_style_position(v: &str) -> Option<ListStylePosition> {
    match v {
        "outside" => Some(ListStylePosition::Outside),
        "inside" => Some(ListStylePosition::Inside),
        _ => None,
    }
}

/// Parses one `grid-template-columns`/`grid-template-rows` track token —
/// `px` lengths and `fr` shares only, see [`GridTrackSize`]'s own doc.
fn parse_track_size(token: &Token) -> Option<GridTrackSize> {
    match token {
        Token::Dimension(n, unit) if unit == "px" => Some(GridTrackSize::Px(*n)),
        Token::Dimension(n, unit) if unit.eq_ignore_ascii_case("fr") => Some(GridTrackSize::Fr(*n)),
        Token::Number(n) if *n == 0.0 => Some(GridTrackSize::Px(0.0)),
        _ => None,
    }
}

pub(super) fn apply_declaration(style: &mut ComputedStyle, decl: &Declaration) {
    match decl.name.as_str() {
        "display" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                style.display = match v.as_str() {
                    "block" => Display::Block,
                    "inline" => Display::Inline,
                    "flex" => Display::Flex,
                    "grid" => Display::Grid,
                    "none" => Display::None,
                    _ => return,
                };
            }
        }
        "grid-template-columns" => {
            let tracks: Vec<GridTrackSize> =
                decl.value.iter().filter_map(parse_track_size).collect();
            // All-or-nothing, same convention `parse_edge_shorthand` already
            // uses: a value with one bad track is invalid as a whole.
            if tracks.len() == decl.value.len() && !tracks.is_empty() {
                style.grid_template_columns = GridTracks::from_iter_capped(tracks.into_iter());
            }
        }
        "grid-template-rows" => {
            let tracks: Vec<GridTrackSize> =
                decl.value.iter().filter_map(parse_track_size).collect();
            if tracks.len() == decl.value.len() && !tracks.is_empty() {
                style.grid_template_rows = GridTracks::from_iter_capped(tracks.into_iter());
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
            if is_linear_gradient_call(&decl.value) {
                if let Some(g) = parse_linear_gradient(linear_gradient_inner(&decl.value)) {
                    style.background_image = Some(g);
                }
                return;
            }
            let color = match decl.value.first() {
                Some(Token::Ident(name)) => Color::named(name),
                Some(Token::Hash(hex)) => Color::from_hex(hex),
                _ => None,
            };
            if let Some(c) = color {
                style.background_color = c;
            }
        }
        "background-image" => {
            if is_linear_gradient_call(&decl.value) {
                if let Some(g) = parse_linear_gradient(linear_gradient_inner(&decl.value)) {
                    style.background_image = Some(g);
                }
            }
            // `url(...)` (a real image background) isn't modeled - same
            // "solid color only" scope cut `"background"`'s own arm above
            // already documents.
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
                    "fixed" => Position::Fixed,
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
        "font-family" => {
            if let Some(f) = parse_font_family(&decl.value) {
                style.font_family = Some(f);
            }
        }
        "list-style-type" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                if let Some(t) = parse_list_style_type(v) {
                    style.list_style_type = Some(t);
                }
            }
        }
        "list-style-position" => {
            if let Some(Token::Ident(v)) = decl.value.first() {
                if let Some(p) = parse_list_style_position(v) {
                    style.list_style_position = p;
                }
            }
        }
        "list-style" => {
            // Real `list-style` also accepts a `list-style-image` (a
            // `url(...)`), which this crate doesn't model — same
            // "background-color only, no background-image" scope cut
            // `apply_declaration`'s `"background"` arm above already has.
            // Any `Ident` token here can only be a type or position
            // keyword, so both are read from the same token list.
            for token in &decl.value {
                if let Token::Ident(v) = token {
                    if let Some(t) = parse_list_style_type(v) {
                        style.list_style_type = Some(t);
                    } else if let Some(p) = parse_list_style_position(v) {
                        style.list_style_position = p;
                    }
                }
            }
        }
        _ => {}
    }
}
