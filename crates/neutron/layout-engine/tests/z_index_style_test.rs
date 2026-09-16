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
fn z_index_defaults_to_auto() {
    let sheet = parse_stylesheet("div { width: 10px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.z_index, None);
}

#[test]
fn z_index_resolves_a_positive_integer() {
    let sheet = parse_stylesheet("div { z-index: 5; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.z_index, Some(5));
}

#[test]
fn z_index_resolves_a_negative_integer() {
    let sheet = parse_stylesheet("div { z-index: -3; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.z_index, Some(-3));
}

#[test]
fn z_index_auto_clears_a_previously_cascaded_value() {
    let sheet = parse_stylesheet("div { z-index: 5; } #a { z-index: auto; }");
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
    assert_eq!(style.z_index, None);
}
