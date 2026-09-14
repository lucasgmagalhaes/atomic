use css::{matching_declarations, parse_stylesheet, ElementSnapshot};
use layout_engine::{resolve_style, Color};

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

fn style_for(css: &str) -> layout_engine::ComputedStyle {
    let sheet = parse_stylesheet(css);
    resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    )
}

#[test]
fn no_background_image_by_default() {
    let style = style_for("div { background-color: red; }");
    assert!(style.background_image.is_none());
}

#[test]
fn linear_gradient_with_explicit_angle_and_two_colors() {
    let style = style_for("div { background: linear-gradient(90deg, red, blue); }");
    let g = style.background_image.expect("expected a gradient");
    assert_eq!(g.angle_deg, 90.0);
    assert_eq!(
        g.from,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
    assert_eq!(
        g.to,
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }
    );
}

#[test]
fn linear_gradient_defaults_to_180_degrees_when_no_direction_is_given() {
    let style = style_for("div { background: linear-gradient(red, blue); }");
    let g = style.background_image.expect("expected a gradient");
    assert_eq!(g.angle_deg, 180.0);
}

#[test]
fn linear_gradient_to_right_keyword_resolves_to_90_degrees() {
    let style = style_for("div { background: linear-gradient(to right, red, blue); }");
    let g = style.background_image.expect("expected a gradient");
    assert_eq!(g.angle_deg, 90.0);
}

#[test]
fn linear_gradient_with_more_than_two_stops_keeps_only_the_first_and_last() {
    let style = style_for("div { background: linear-gradient(red, green, blue); }");
    let g = style.background_image.expect("expected a gradient");
    assert_eq!(
        g.from,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
    assert_eq!(
        g.to,
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }
    );
}

#[test]
fn background_image_longhand_also_parses_a_gradient() {
    let style = style_for("div { background-image: linear-gradient(to left, #fff, #000); }");
    let g = style.background_image.expect("expected a gradient");
    assert_eq!(g.angle_deg, 270.0);
}

#[test]
fn a_plain_solid_background_still_sets_no_gradient() {
    let style = style_for("div { background: #336699; }");
    assert!(style.background_image.is_none());
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
