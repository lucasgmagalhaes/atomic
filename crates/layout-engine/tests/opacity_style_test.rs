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
fn opacity_defaults_to_fully_opaque() {
    let sheet = parse_stylesheet("div { width: 10px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.opacity, 1.0);
}

#[test]
fn opacity_resolves_a_plain_number() {
    let sheet = parse_stylesheet("div { opacity: 0.5; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.opacity, 0.5);
}

#[test]
fn opacity_resolves_a_percentage() {
    let sheet = parse_stylesheet("div { opacity: 25%; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.opacity, 0.25);
}

#[test]
fn opacity_above_one_is_clamped_to_one() {
    // Negative unitless numbers aren't exercised here - this hand-rolled
    // CSS lexer doesn't tokenize a leading `-` before a digit as part of
    // a `Number` token at all (a pre-existing, unrelated gap - "-1" lexes
    // as an `Ident`, not a signed number, anywhere in this crate, not
    // just for `opacity`), so there's no real negative value to clamp in
    // the first place for this property today.
    let sheet = parse_stylesheet("div { opacity: 2; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.opacity, 1.0);

    let sheet = parse_stylesheet("div { opacity: 150%; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.opacity, 1.0);
}
