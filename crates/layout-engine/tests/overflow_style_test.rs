use css::{matching_declarations, parse_stylesheet, ElementSnapshot};
use layout_engine::{resolve_style, Color, Overflow};

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
fn overflow_defaults_to_visible() {
    let sheet = parse_stylesheet("div { width: 10px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.overflow, Overflow::Visible);
}

#[test]
fn overflow_hidden_auto_and_scroll_all_resolve_to_hidden() {
    for value in ["hidden", "auto", "scroll"] {
        let sheet = parse_stylesheet(&format!("div {{ overflow: {value}; }}"));
        let style = resolve_style(
            &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
            16.0,
            BLACK,
        );
        assert_eq!(
            style.overflow,
            Overflow::Hidden,
            "overflow: {value} should resolve to Hidden"
        );
    }
}

#[test]
fn overflow_visible_resolves_to_visible() {
    let sheet = parse_stylesheet("div { overflow: visible; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.overflow, Overflow::Visible);
}
