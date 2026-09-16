use css::{matching_declarations, parse_stylesheet, ElementSnapshot};
use layout_engine::{resolve_style, BoxShadow, Color};

const BLACK: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};

fn el(tag: &str) -> ElementSnapshot<'_> {
    ElementSnapshot {
        tag,
        id: None,
        classes: vec![],
        ..Default::default()
    }
}

#[test]
fn box_shadow_defaults_to_none() {
    let sheet = parse_stylesheet("div { width: 10px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.box_shadow, None);
}

#[test]
fn box_shadow_two_value_form_is_offset_only_with_a_color() {
    let sheet = parse_stylesheet("div { box-shadow: 5px 10px red; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(
        style.box_shadow,
        Some(BoxShadow {
            offset_x: 5.0,
            offset_y: 10.0,
            spread: 0.0,
            color: Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            },
        })
    );
}

#[test]
fn box_shadow_four_value_form_skips_blur_and_keeps_spread() {
    // offset-x offset-y blur-radius spread-radius color - the 10px blur
    // is parsed (so it isn't mistaken for spread) but not stored.
    let sheet = parse_stylesheet("div { box-shadow: 2px 3px 10px 4px #00ff00; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(
        style.box_shadow,
        Some(BoxShadow {
            offset_x: 2.0,
            offset_y: 3.0,
            spread: 4.0,
            color: Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255
            },
        })
    );
}

#[test]
fn box_shadow_none_clears_a_previously_cascaded_shadow() {
    let sheet = parse_stylesheet("div { box-shadow: 5px 5px red; } #a { box-shadow: none; }");
    let style = resolve_style(
        &matching_declarations(
            &sheet,
            &[ElementSnapshot {
                tag: "div",
                id: Some("a"),
                classes: vec![],
                ..Default::default()
            }],
            1024.0,
            768.0,
        ),
        16.0,
        BLACK,
    );
    assert_eq!(style.box_shadow, None);
}

#[test]
fn box_shadow_without_a_color_is_not_set() {
    let sheet = parse_stylesheet("div { box-shadow: 5px 5px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.box_shadow, None);
}
