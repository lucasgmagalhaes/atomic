//! Real coverage for `SelectorIndex`/`build_selector_index`/
//! `matching_declarations_indexed` (`crates/css/src/cascade.rs`) — proves
//! the indexed path returns exactly the same result as the unindexed
//! `matching_declarations` for id/class/tag/universal-keyed selectors,
//! combinators, and multi-selector rules.
use css::{
    build_selector_index, matching_declarations, matching_declarations_indexed, parse_stylesheet,
    ElementSnapshot,
};

fn el(tag: &str) -> ElementSnapshot<'_> {
    ElementSnapshot {
        tag,
        ..Default::default()
    }
}

fn el_with_id<'a>(tag: &'a str, id: &'a str) -> ElementSnapshot<'a> {
    ElementSnapshot {
        tag,
        id: Some(id),
        ..Default::default()
    }
}

fn el_with_classes<'a>(tag: &'a str, classes: Vec<&'a str>) -> ElementSnapshot<'a> {
    ElementSnapshot {
        tag,
        classes,
        ..Default::default()
    }
}

fn declared_properties(matched: &[css::MatchedDeclarations]) -> Vec<String> {
    matched
        .iter()
        .flat_map(|m| m.declarations.iter().map(|d| d.name.clone()))
        .collect()
}

#[test]
fn id_keyed_selector_matches_via_the_index() {
    let sheet = parse_stylesheet("#a { color: red; }");
    let index = build_selector_index(&sheet);
    let chain = vec![el_with_id("div", "a")];

    let direct = matching_declarations(&sheet, &chain, 1024.0, 768.0);
    let indexed = matching_declarations_indexed(&index, &sheet, &chain, 1024.0, 768.0);
    assert_eq!(declared_properties(&direct), declared_properties(&indexed));
    assert_eq!(declared_properties(&indexed), vec!["color"]);
}

#[test]
fn class_keyed_selector_matches_via_the_index_for_any_of_the_elements_classes() {
    let sheet = parse_stylesheet(".foo { color: red; } .bar { width: 10px; }");
    let index = build_selector_index(&sheet);
    let chain = vec![el_with_classes("div", vec!["foo", "bar"])];

    let direct = matching_declarations(&sheet, &chain, 1024.0, 768.0);
    let indexed = matching_declarations_indexed(&index, &sheet, &chain, 1024.0, 768.0);
    assert_eq!(
        declared_properties(&direct).len(),
        declared_properties(&indexed).len()
    );
    let mut props = declared_properties(&indexed);
    props.sort();
    assert_eq!(props, vec!["color", "width"]);
}

#[test]
fn tag_keyed_selector_matches_via_the_index() {
    let sheet = parse_stylesheet("span { color: blue; }");
    let index = build_selector_index(&sheet);
    let chain = vec![el("span")];

    let direct = matching_declarations(&sheet, &chain, 1024.0, 768.0);
    let indexed = matching_declarations_indexed(&index, &sheet, &chain, 1024.0, 768.0);
    assert_eq!(declared_properties(&direct), declared_properties(&indexed));
    assert_eq!(declared_properties(&indexed), vec!["color"]);
}

#[test]
fn a_selector_with_no_id_class_or_tag_falls_into_the_universal_bucket_and_still_matches() {
    let sheet = parse_stylesheet("* { color: green; }");
    let index = build_selector_index(&sheet);
    let chain = vec![el("section")];

    let direct = matching_declarations(&sheet, &chain, 1024.0, 768.0);
    let indexed = matching_declarations_indexed(&index, &sheet, &chain, 1024.0, 768.0);
    assert_eq!(declared_properties(&direct), declared_properties(&indexed));
    assert_eq!(declared_properties(&indexed), vec!["color"]);
}

#[test]
fn a_selector_keyed_on_id_still_uses_the_full_compound_to_reject_a_wrong_tag() {
    // Rightmost compound is `div#a` - bucketed under id "a", but the tag
    // must still be checked (compound_matches checks every simple
    // selector in the compound, not just the one used for bucketing).
    let sheet = parse_stylesheet("div#a { color: red; }");
    let index = build_selector_index(&sheet);
    let wrong_tag = vec![el_with_id("span", "a")];
    let right_tag = vec![el_with_id("div", "a")];

    assert!(matching_declarations_indexed(&index, &sheet, &wrong_tag, 1024.0, 768.0).is_empty());
    assert_eq!(
        declared_properties(&matching_declarations_indexed(
            &index, &sheet, &right_tag, 1024.0, 768.0
        )),
        vec!["color"]
    );
}

#[test]
fn descendant_combinator_still_matches_through_the_index() {
    let sheet = parse_stylesheet("article span { color: red; }");
    let index = build_selector_index(&sheet);
    let chain = vec![el("article"), el("span")];

    let direct = matching_declarations(&sheet, &chain, 1024.0, 768.0);
    let indexed = matching_declarations_indexed(&index, &sheet, &chain, 1024.0, 768.0);
    assert_eq!(declared_properties(&direct), declared_properties(&indexed));
    assert_eq!(declared_properties(&indexed), vec!["color"]);
}

#[test]
fn descendant_combinator_does_not_falsely_match_through_the_index_without_the_right_ancestor() {
    let sheet = parse_stylesheet("article span { color: red; }");
    let index = build_selector_index(&sheet);
    let chain = vec![el("section"), el("span")];

    assert!(matching_declarations_indexed(&index, &sheet, &chain, 1024.0, 768.0).is_empty());
}

#[test]
fn one_rule_matched_by_two_comma_separated_selectors_contributes_once_at_the_highest_specificity() {
    let sheet = parse_stylesheet("span, #a { color: red; }");
    let index = build_selector_index(&sheet);
    let chain = vec![el_with_id("span", "a")];

    let direct = matching_declarations(&sheet, &chain, 1024.0, 768.0);
    let indexed = matching_declarations_indexed(&index, &sheet, &chain, 1024.0, 768.0);
    assert_eq!(direct.len(), 1);
    assert_eq!(indexed.len(), 1);
    assert_eq!(direct[0].specificity, indexed[0].specificity);
}

#[test]
fn a_media_query_that_does_not_match_the_viewport_excludes_the_rule_via_the_index_too() {
    let sheet = parse_stylesheet("@media (min-width: 2000px) { #a { color: red; } }");
    let index = build_selector_index(&sheet);
    let chain = vec![el_with_id("div", "a")];

    assert!(matching_declarations_indexed(&index, &sheet, &chain, 1024.0, 768.0).is_empty());
    assert_eq!(
        matching_declarations_indexed(&index, &sheet, &chain, 2500.0, 768.0).len(),
        1
    );
}
