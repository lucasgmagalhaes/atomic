use css::{matching_declarations, parse_stylesheet, ElementSnapshot};
use layout_engine::{resolve_style, BoxShadow, Color, Display, Length, Overflow};

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
fn resolves_to_initial_values_when_nothing_matches() {
    let sheet = parse_stylesheet("span { color: red; }");
    let chain = vec![el("div")];
    let matched = matching_declarations(&sheet, &chain, 1024.0, 768.0);
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
    let matched = matching_declarations(&sheet, &chain, 1024.0, 768.0);
    let style = resolve_style(&matched, 16.0, BLACK);

    assert_eq!(style.width, Length::Px(100.0));
    assert_eq!(style.height, Length::Percent(50.0));
    assert_eq!(style.display, Display::Inline);
}

#[test]
fn resolves_margin_shorthand_one_value() {
    let sheet = parse_stylesheet("div { margin: 10px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.margin.top, Length::Px(10.0));
    assert_eq!(style.margin.right, Length::Px(10.0));
    assert_eq!(style.margin.bottom, Length::Px(10.0));
    assert_eq!(style.margin.left, Length::Px(10.0));
}

#[test]
fn resolves_margin_shorthand_four_values() {
    let sheet = parse_stylesheet("div { margin: 1px 2px 3px 4px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.margin.top, Length::Px(1.0));
    assert_eq!(style.margin.right, Length::Px(2.0));
    assert_eq!(style.margin.bottom, Length::Px(3.0));
    assert_eq!(style.margin.left, Length::Px(4.0));
}

#[test]
fn longhand_overrides_shorthand_when_cascaded_later() {
    let sheet = parse_stylesheet("div { margin: 10px; margin-left: 99px; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.margin.left, Length::Px(99.0));
    assert_eq!(style.margin.top, Length::Px(10.0));
}

#[test]
fn higher_specificity_wins_the_cascade() {
    let sheet = parse_stylesheet("div { width: 10px; } #id { width: 20px; }");
    let chain = vec![ElementSnapshot {
        tag: "div",
        id: Some("id"),
        classes: vec![],
        ..Default::default()
    }];
    let style = resolve_style(
        &matching_declarations(&sheet, &chain, 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.width, Length::Px(20.0));
}

#[test]
fn unitless_zero_is_a_valid_length() {
    let sheet = parse_stylesheet("div { margin: 0; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    assert_eq!(style.margin.top, Length::Px(0.0));
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
