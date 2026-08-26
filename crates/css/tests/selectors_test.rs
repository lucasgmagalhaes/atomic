//! Real coverage for child/sibling combinators, attribute selectors, and
//! structural pseudo-classes (`crates/css/src/parser.rs`/`cascade.rs`).
use css::{parse_stylesheet, selector_matches, ElementSnapshot};

fn el(tag: &str) -> ElementSnapshot<'_> {
    ElementSnapshot {
        tag,
        ..Default::default()
    }
}

fn el_with_attrs<'a>(tag: &'a str, attrs: &[(&'a str, &'a str)]) -> ElementSnapshot<'a> {
    ElementSnapshot {
        tag,
        attributes: attrs.to_vec(),
        ..Default::default()
    }
}

/// Builds a target `ElementSnapshot` with `n` preceding element siblings
/// (all a plain `<li>` with no attributes) and whether a following one
/// exists — for `:first-child`/`:last-child`/`:nth-child` tests.
fn el_at_position(tag: &str, preceding_count: usize, has_following: bool) -> ElementSnapshot<'_> {
    ElementSnapshot {
        tag,
        preceding_siblings: (0..preceding_count).map(|_| el("li")).collect(),
        has_following_sibling: has_following,
        ..Default::default()
    }
}

// --- Child combinator ---

#[test]
fn child_combinator_matches_immediate_parent() {
    let sheet = parse_stylesheet("div > span {}");
    let chain = vec![el("div"), el("span")];
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &chain));
}

#[test]
fn child_combinator_does_not_match_a_grandparent() {
    let sheet = parse_stylesheet("div > span {}");
    let chain = vec![el("div"), el("section"), el("span")];
    assert!(!selector_matches(&sheet.rules[0].selectors.0[0], &chain));
}

#[test]
fn child_combinator_can_be_mixed_with_descendant() {
    let sheet = parse_stylesheet("article div > span {}");
    // article (ancestor, anywhere above) > div (immediate parent) > span (target)
    let matching = vec![el("article"), el("section"), el("div"), el("span")];
    let not_matching = vec![el("article"), el("div"), el("section"), el("span")];
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &matching));
    assert!(!selector_matches(
        &sheet.rules[0].selectors.0[0],
        &not_matching
    ));
}

// --- Sibling combinators ---

#[test]
fn next_sibling_matches_the_immediately_preceding_sibling() {
    let sheet = parse_stylesheet("h1 + p {}");
    let mut target = el("p");
    target.preceding_siblings = vec![el("h1")];
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &[target]));
}

#[test]
fn next_sibling_does_not_match_a_non_adjacent_earlier_sibling() {
    let sheet = parse_stylesheet("h1 + p {}");
    let mut target = el("p");
    target.preceding_siblings = vec![el("h1"), el("span")];
    assert!(!selector_matches(&sheet.rules[0].selectors.0[0], &[target]));
}

#[test]
fn subsequent_sibling_matches_any_earlier_sibling() {
    let sheet = parse_stylesheet("h1 ~ p {}");
    let mut target = el("p");
    target.preceding_siblings = vec![el("h1"), el("span")];
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &[target]));
}

#[test]
fn subsequent_sibling_requires_at_least_one_matching_earlier_sibling() {
    let sheet = parse_stylesheet("h1 ~ p {}");
    let mut target = el("p");
    target.preceding_siblings = vec![el("span"), el("em")];
    assert!(!selector_matches(&sheet.rules[0].selectors.0[0], &[target]));
}

#[test]
fn chained_sibling_combinators_walk_the_same_flat_sibling_list() {
    let sheet = parse_stylesheet("a + b + c {}");
    let mut target = el("c");
    target.preceding_siblings = vec![el("a"), el("b")];
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &[target]));

    let mut wrong_order = el("c");
    wrong_order.preceding_siblings = vec![el("b"), el("a")];
    assert!(!selector_matches(
        &sheet.rules[0].selectors.0[0],
        &[wrong_order]
    ));
}

// --- Attribute selectors ---

#[test]
fn has_attribute_selector_matches_regardless_of_value() {
    let sheet = parse_stylesheet("[disabled] {}");
    let with_attr = el_with_attrs("input", &[("disabled", "")]);
    let without_attr = el_with_attrs("input", &[]);
    assert!(selector_matches(
        &sheet.rules[0].selectors.0[0],
        &[with_attr]
    ));
    assert!(!selector_matches(
        &sheet.rules[0].selectors.0[0],
        &[without_attr]
    ));
}

#[test]
fn attribute_equals_selector_requires_exact_value_match() {
    let sheet = parse_stylesheet(r#"[type="text"] {}"#);
    let matching = el_with_attrs("input", &[("type", "text")]);
    let not_matching = el_with_attrs("input", &[("type", "checkbox")]);
    assert!(selector_matches(
        &sheet.rules[0].selectors.0[0],
        &[matching]
    ));
    assert!(!selector_matches(
        &sheet.rules[0].selectors.0[0],
        &[not_matching]
    ));
}

#[test]
fn attribute_selector_combines_with_type_selector() {
    let sheet = parse_stylesheet(r#"input[type="text"] {}"#);
    let matching = el_with_attrs("input", &[("type", "text")]);
    let wrong_tag = el_with_attrs("textarea", &[("type", "text")]);
    assert!(selector_matches(
        &sheet.rules[0].selectors.0[0],
        &[matching]
    ));
    assert!(!selector_matches(
        &sheet.rules[0].selectors.0[0],
        &[wrong_tag]
    ));
}

// --- Structural pseudo-classes ---

#[test]
fn first_child_matches_only_with_no_preceding_siblings() {
    let sheet = parse_stylesheet("li:first-child {}");
    let first = el_at_position("li", 0, true);
    let second = el_at_position("li", 1, true);
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &[first]));
    assert!(!selector_matches(&sheet.rules[0].selectors.0[0], &[second]));
}

#[test]
fn last_child_matches_only_with_no_following_sibling() {
    let sheet = parse_stylesheet("li:last-child {}");
    let last = el_at_position("li", 2, false);
    let middle = el_at_position("li", 1, true);
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &[last]));
    assert!(!selector_matches(&sheet.rules[0].selectors.0[0], &[middle]));
}

#[test]
fn nth_child_plain_integer_matches_exact_position() {
    let sheet = parse_stylesheet("li:nth-child(3) {}");
    let third = el_at_position("li", 2, true); // position 3 (1-based)
    let second = el_at_position("li", 1, true);
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &[third]));
    assert!(!selector_matches(&sheet.rules[0].selectors.0[0], &[second]));
}

#[test]
fn nth_child_odd_matches_positions_one_three_five() {
    let sheet = parse_stylesheet("li:nth-child(odd) {}");
    for (preceding, expected) in [(0, true), (1, false), (2, true), (3, false), (4, true)] {
        let snapshot = el_at_position("li", preceding, true);
        assert_eq!(
            selector_matches(&sheet.rules[0].selectors.0[0], &[snapshot]),
            expected,
            "position {} (0-based preceding count {preceding})",
            preceding + 1
        );
    }
}

#[test]
fn nth_child_even_matches_positions_two_four() {
    let sheet = parse_stylesheet("li:nth-child(even) {}");
    let second = el_at_position("li", 1, true);
    let third = el_at_position("li", 2, true);
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &[second]));
    assert!(!selector_matches(&sheet.rules[0].selectors.0[0], &[third]));
}

#[test]
fn nth_child_an_plus_b_formula_matches_the_arithmetic_sequence() {
    let sheet = parse_stylesheet("li:nth-child(2n+1) {}");
    // 2n+1: positions 1, 3, 5, ...
    for (preceding, expected) in [(0, true), (1, false), (2, true), (3, false)] {
        let snapshot = el_at_position("li", preceding, true);
        assert_eq!(
            selector_matches(&sheet.rules[0].selectors.0[0], &[snapshot]),
            expected
        );
    }
}

#[test]
fn nth_child_bare_n_matches_every_position() {
    let sheet = parse_stylesheet("li:nth-child(n) {}");
    for preceding in 0..5 {
        let snapshot = el_at_position("li", preceding, true);
        assert!(selector_matches(
            &sheet.rules[0].selectors.0[0],
            &[snapshot]
        ));
    }
}

// --- Honest hover/focus stand-in ---

#[test]
fn hover_and_focus_pseudo_classes_parse_but_never_match() {
    let sheet = parse_stylesheet("a:hover {} input:focus {}");
    assert_eq!(sheet.rules.len(), 2);
    let anchor = el("a");
    let input = el("input");
    assert!(!selector_matches(&sheet.rules[0].selectors.0[0], &[anchor]));
    assert!(!selector_matches(&sheet.rules[1].selectors.0[0], &[input]));
}

// --- Specificity ---

#[test]
fn attribute_and_pseudo_class_selectors_count_as_class_specificity() {
    let sheet = parse_stylesheet("[disabled] {} li:first-child {} div {}");
    let specs: Vec<_> = sheet
        .rules
        .iter()
        .map(|r| r.selectors.0[0].specificity())
        .collect();
    assert_eq!(specs[0], (0, 1, 0)); // [disabled]
    assert_eq!(specs[1], (0, 1, 1)); // li:first-child -> type + class tier
    assert_eq!(specs[2], (0, 0, 1)); // div
}
