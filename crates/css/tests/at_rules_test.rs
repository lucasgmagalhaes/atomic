use css::{matching_declarations, parse_stylesheet, ElementSnapshot};

fn el(tag: &str) -> ElementSnapshot {
    ElementSnapshot { tag: tag.to_string(), ..Default::default() }
}

#[test]
fn media_min_width_rule_only_applies_at_or_above_the_breakpoint() {
    let sheet = parse_stylesheet("@media (min-width: 600px) { div { color: red; } }");
    let chain = [el("div")];

    assert!(matching_declarations(&sheet, &chain, 599.0).is_empty());
    assert_eq!(matching_declarations(&sheet, &chain, 600.0).len(), 1);
    assert_eq!(matching_declarations(&sheet, &chain, 1200.0).len(), 1);
}

#[test]
fn media_max_width_rule_only_applies_at_or_below_the_breakpoint() {
    let sheet = parse_stylesheet("@media (max-width: 600px) { div { color: blue; } }");
    let chain = [el("div")];

    assert_eq!(matching_declarations(&sheet, &chain, 600.0).len(), 1);
    assert!(matching_declarations(&sheet, &chain, 601.0).is_empty());
}

#[test]
fn media_query_combines_min_and_max_with_and() {
    let sheet = parse_stylesheet("@media (min-width: 400px) and (max-width: 800px) { div { color: green; } }");
    let chain = [el("div")];

    assert!(matching_declarations(&sheet, &chain, 399.0).is_empty());
    assert_eq!(matching_declarations(&sheet, &chain, 400.0).len(), 1);
    assert_eq!(matching_declarations(&sheet, &chain, 800.0).len(), 1);
    assert!(matching_declarations(&sheet, &chain, 801.0).is_empty());
}

#[test]
fn media_type_screen_and_all_match_but_print_never_does() {
    let screen = parse_stylesheet("@media screen { div { color: red; } }");
    let all = parse_stylesheet("@media all { div { color: red; } }");
    let print = parse_stylesheet("@media print { div { color: red; } }");
    let chain = [el("div")];

    assert_eq!(matching_declarations(&screen, &chain, 1024.0).len(), 1);
    assert_eq!(matching_declarations(&all, &chain, 1024.0).len(), 1);
    assert!(matching_declarations(&print, &chain, 1024.0).is_empty(), "this engine never renders print, so @media print should never match");
}

#[test]
fn rules_outside_any_media_block_are_unaffected() {
    let sheet = parse_stylesheet("div { color: black; } @media (min-width: 9999px) { div { color: red; } }");
    let chain = [el("div")];

    // Only the unconditional rule should match at a narrow viewport.
    assert_eq!(matching_declarations(&sheet, &chain, 100.0).len(), 1);
}

#[test]
fn import_with_a_quoted_url_is_collected() {
    let sheet = parse_stylesheet(r#"@import "theme.css";"#);
    assert_eq!(sheet.imports.len(), 1);
    assert_eq!(sheet.imports[0].url, "theme.css");
    assert!(sheet.imports[0].media.is_none());
}

#[test]
fn import_with_a_quoted_url_function_form_is_collected() {
    let sheet = parse_stylesheet(r#"@import url("theme.css");"#);
    assert_eq!(sheet.imports.len(), 1);
    assert_eq!(sheet.imports[0].url, "theme.css");
}

#[test]
fn import_with_a_trailing_media_query_is_collected() {
    let sheet = parse_stylesheet(r#"@import "wide.css" screen and (min-width: 600px);"#);
    assert_eq!(sheet.imports.len(), 1);
    let media = sheet.imports[0].media.as_ref().unwrap();
    assert!(media.matches(600.0));
    assert!(!media.matches(599.0));
}

#[test]
fn multiple_imports_and_a_following_rule_all_parse() {
    let sheet = parse_stylesheet(r#"@import "a.css"; @import "b.css"; div { color: red; }"#);
    assert_eq!(sheet.imports.len(), 2);
    assert_eq!(sheet.imports[0].url, "a.css");
    assert_eq!(sheet.imports[1].url, "b.css");
    assert_eq!(sheet.rules.len(), 1);
}

#[test]
fn unknown_at_rules_are_skipped_without_corrupting_the_rest_of_the_sheet() {
    let sheet = parse_stylesheet(
        r#"
        @font-face { font-family: "Foo"; src: url("foo.woff"); }
        @keyframes spin { from { color: red; } to { color: blue; } }
        @charset "utf-8";
        div { color: green; }
        "#,
    );
    assert_eq!(sheet.rules.len(), 1);
    assert_eq!(sheet.rules[0].declarations[0].name, "color");
}
