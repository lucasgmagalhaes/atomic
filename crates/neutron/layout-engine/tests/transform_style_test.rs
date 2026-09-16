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
fn transform_defaults_to_no_translation() {
    let sheet = parse_stylesheet("div { width: 10px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.transform, (0.0, 0.0));
}

#[test]
fn transform_translate_resolves_both_axes() {
    let sheet = parse_stylesheet("div { transform: translate(10px, 20px); }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.transform, (10.0, 20.0));
}

#[test]
fn transform_translate_with_negative_values() {
    let sheet = parse_stylesheet("div { transform: translate(-10px, -20px); }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.transform, (-10.0, -20.0));
}

#[test]
fn transform_translatex_and_translatey_compose() {
    let sheet = parse_stylesheet("div { transform: translateX(5px) translateY(-7px); }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.transform, (5.0, -7.0));
}

#[test]
fn transform_none_clears_a_previously_cascaded_value() {
    let sheet =
        parse_stylesheet("div { transform: translate(10px, 10px); } #a { transform: none; }");
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
    assert_eq!(style.transform, (0.0, 0.0));
}

#[test]
fn transform_ignores_an_unsupported_function_but_stays_synchronized() {
    let sheet = parse_stylesheet("div { transform: rotate(45deg) translate(3px, 4px); }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(
        style.transform,
        (3.0, 4.0),
        "rotate() must not offset anything, but the translate() after it must still parse"
    );
}
