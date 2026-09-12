use css::{matching_declarations, parse_stylesheet, ElementSnapshot};
use layout_engine::{resolve_style, Color};

fn style_for(css_text: &str) -> layout_engine::ComputedStyle {
    let sheet = parse_stylesheet(css_text);
    let chain = vec![ElementSnapshot {
        tag: "div",
        id: None,
        classes: vec![],
        ..Default::default()
    }];
    resolve_style(
        &matching_declarations(&sheet, &chain, 1024.0, 768.0),
        16.0,
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
    )
}

#[test]
fn defaults_to_transparent() {
    let style = style_for("");
    assert_eq!(style.background_color, Color::TRANSPARENT);
}

#[test]
fn parses_named_colors() {
    let style = style_for("div { background-color: red; }");
    assert_eq!(
        style.background_color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn parses_background_shorthand_as_solid_color() {
    let style = style_for("div { background: blue; }");
    assert_eq!(
        style.background_color,
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }
    );
}

#[test]
fn parses_hex_rrggbb() {
    let style = style_for("div { background-color: #336699; }");
    assert_eq!(
        style.background_color,
        Color {
            r: 0x33,
            g: 0x66,
            b: 0x99,
            a: 255
        }
    );
}

#[test]
fn parses_hex_rgb_shorthand_expanding_each_digit() {
    let style = style_for("div { background-color: #f0a; }");
    assert_eq!(
        style.background_color,
        Color {
            r: 0xff,
            g: 0x00,
            b: 0xaa,
            a: 255
        }
    );
}

#[test]
fn invalid_color_falls_back_to_previous_value() {
    let style = style_for("div { background-color: red; background-color: not-a-color; }");
    assert_eq!(
        style.background_color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}
