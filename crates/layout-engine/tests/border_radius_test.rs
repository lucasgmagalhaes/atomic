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

#[test]
fn border_radius_defaults_to_zero() {
    let sheet = parse_stylesheet("div { width: 10px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.border_radius, 0.0);
}

#[test]
fn border_radius_length_value_is_stored() {
    let sheet = parse_stylesheet("div { border-radius: 12px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.border_radius, 12.0);
}

#[test]
fn border_radius_zero_clears_a_previously_cascaded_value() {
    let sheet = parse_stylesheet("div { border-radius: 12px; } #a { border-radius: 0; }");
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
        ),
        16.0,
        BLACK,
    );
    assert_eq!(style.border_radius, 0.0);
}
