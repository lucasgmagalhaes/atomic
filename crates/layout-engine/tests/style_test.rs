use css::{matching_declarations, parse_stylesheet, ElementSnapshot};
use layout_engine::{resolve_style, Color, Display, Length};

const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };

fn el(tag: &str) -> ElementSnapshot {
    ElementSnapshot {
        tag: tag.to_string(),
        id: None,
        classes: vec![],
    }
}

#[test]
fn resolves_to_initial_values_when_nothing_matches() {
    let sheet = parse_stylesheet("span { color: red; }");
    let chain = vec![el("div")];
    let matched = matching_declarations(&sheet, &chain, 1024.0);
    let style = resolve_style(&matched, 16.0, BLACK);

    assert_eq!(style.display, Display::Block);
    assert_eq!(style.width, Length::Auto);
    assert_eq!(style.height, Length::Auto);
    assert_eq!(style.margin.top, Length::Px(0.0));
}

#[test]
fn resolves_width_height_and_display() {
    let sheet = parse_stylesheet("div { width: 100px; height: 50%; display: inline; }");
    let chain = vec![el("div")];
    let matched = matching_declarations(&sheet, &chain, 1024.0);
    let style = resolve_style(&matched, 16.0, BLACK);

    assert_eq!(style.width, Length::Px(100.0));
    assert_eq!(style.height, Length::Percent(50.0));
    assert_eq!(style.display, Display::Inline);
}

#[test]
fn resolves_margin_shorthand_one_value() {
    let sheet = parse_stylesheet("div { margin: 10px; }");
    let style = resolve_style(&matching_declarations(&sheet, &[el("div")], 1024.0), 16.0, BLACK);
    assert_eq!(style.margin.top, Length::Px(10.0));
    assert_eq!(style.margin.right, Length::Px(10.0));
    assert_eq!(style.margin.bottom, Length::Px(10.0));
    assert_eq!(style.margin.left, Length::Px(10.0));
}

#[test]
fn resolves_margin_shorthand_four_values() {
    let sheet = parse_stylesheet("div { margin: 1px 2px 3px 4px; }");
    let style = resolve_style(&matching_declarations(&sheet, &[el("div")], 1024.0), 16.0, BLACK);
    assert_eq!(style.margin.top, Length::Px(1.0));
    assert_eq!(style.margin.right, Length::Px(2.0));
    assert_eq!(style.margin.bottom, Length::Px(3.0));
    assert_eq!(style.margin.left, Length::Px(4.0));
}

#[test]
fn longhand_overrides_shorthand_when_cascaded_later() {
    let sheet = parse_stylesheet("div { margin: 10px; margin-left: 99px; }");
    let style = resolve_style(&matching_declarations(&sheet, &[el("div")], 1024.0), 16.0, BLACK);
    assert_eq!(style.margin.left, Length::Px(99.0));
    assert_eq!(style.margin.top, Length::Px(10.0));
}

#[test]
fn higher_specificity_wins_the_cascade() {
    let sheet = parse_stylesheet("div { width: 10px; } #id { width: 20px; }");
    let chain = vec![ElementSnapshot {
        tag: "div".into(),
        id: Some("id".into()),
        classes: vec![],
    }];
    let style = resolve_style(&matching_declarations(&sheet, &chain, 1024.0), 16.0, BLACK);
    assert_eq!(style.width, Length::Px(20.0));
}

#[test]
fn unitless_zero_is_a_valid_length() {
    let sheet = parse_stylesheet("div { margin: 0; }");
    let style = resolve_style(&matching_declarations(&sheet, &[el("div")], 1024.0), 16.0, BLACK);
    assert_eq!(style.margin.top, Length::Px(0.0));
}
