use css::{matching_declarations, parse_stylesheet, selector_matches, ElementSnapshot};

fn el(tag: &str, id: Option<&str>, classes: &[&str]) -> ElementSnapshot {
    ElementSnapshot {
        tag: tag.to_string(),
        id: id.map(str::to_string),
        classes: classes.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn matches_simple_type_selector() {
    let sheet = parse_stylesheet("div {}");
    let chain = vec![el("div", None, &[])];
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &chain));
}

#[test]
fn does_not_match_wrong_type() {
    let sheet = parse_stylesheet("span {}");
    let chain = vec![el("div", None, &[])];
    assert!(!selector_matches(&sheet.rules[0].selectors.0[0], &chain));
}

#[test]
fn matches_id_and_class_together() {
    let sheet = parse_stylesheet("div.item#main {}");
    let matching = el("div", Some("main"), &["item", "other"]);
    let not_matching = el("div", Some("main"), &["other"]);
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
fn matches_descendant_combinator_through_ancestor_chain() {
    let sheet = parse_stylesheet("div .item {}");
    let chain = vec![
        el("body", None, &[]),
        el("div", None, &[]),
        el("span", None, &["item"]),
    ];
    assert!(selector_matches(&sheet.rules[0].selectors.0[0], &chain));
}

#[test]
fn descendant_combinator_requires_ancestor_to_exist() {
    let sheet = parse_stylesheet("section .item {}");
    let chain = vec![el("div", None, &[]), el("span", None, &["item"])];
    assert!(!selector_matches(&sheet.rules[0].selectors.0[0], &chain));
}

#[test]
fn matching_declarations_orders_by_specificity_then_source() {
    let sheet = parse_stylesheet(
        "div { color: red; } \
         .item { color: blue; } \
         #main { color: green; }",
    );
    let chain = vec![el("div", Some("main"), &["item"])];
    let matches = matching_declarations(&sheet, &chain, 1024.0);

    assert_eq!(matches.len(), 3);
    // Lowest specificity first: type (0,0,1), class (0,1,0), id (1,0,0).
    let colors: Vec<_> = matches
        .iter()
        .map(|m| &m.declarations[0].value)
        .collect();
    assert_eq!(colors.len(), 3);
    assert_eq!(matches[0].specificity, (0, 0, 1));
    assert_eq!(matches[1].specificity, (0, 1, 0));
    assert_eq!(matches[2].specificity, (1, 0, 0));
    // Cascade-applying in this order means #main's "green" wins last.
}

#[test]
fn matching_declarations_uses_source_order_as_tiebreak() {
    let sheet = parse_stylesheet(".a { color: red; } .b { color: blue; }");
    let chain = vec![el("div", None, &["a", "b"])];
    let matches = matching_declarations(&sheet, &chain, 1024.0);
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].source_order, 0);
    assert_eq!(matches[1].source_order, 1);
}

#[test]
fn non_matching_rules_are_excluded() {
    let sheet = parse_stylesheet("div {} span {}");
    let chain = vec![el("div", None, &[])];
    let matches = matching_declarations(&sheet, &chain, 1024.0);
    assert_eq!(matches.len(), 1);
}
