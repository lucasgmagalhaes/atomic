//! Real import maps (`Context::set_import_map`) — closes the last
//! remaining sub-item on `spec/matrix/runtime.md`'s ES-module line.

use js_runtime::{Context, Runtime};

#[test]
fn a_bare_specifier_with_no_import_map_still_fails_to_resolve() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert_eq!(
        ctx.resolve_module_specifier("https://example.com/app/main.mjs", "lodash"),
        None
    );
}

#[test]
fn an_exact_import_map_entry_resolves_a_bare_specifier() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.set_import_map(
        "https://example.com/",
        r#"{"imports": {"lodash": "https://cdn.example.com/lodash.mjs"}}"#,
    )
    .expect("valid import map JSON should parse");
    assert_eq!(
        ctx.resolve_module_specifier("https://example.com/app/main.mjs", "lodash"),
        Some("https://cdn.example.com/lodash.mjs".to_string())
    );
}

#[test]
fn a_prefix_import_map_entry_resolves_with_the_remainder_appended() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.set_import_map(
        "https://example.com/",
        r#"{"imports": {"@app/": "/src/app/"}}"#,
    )
    .expect("valid import map JSON should parse");
    assert_eq!(
        ctx.resolve_module_specifier("https://example.com/main.mjs", "@app/utils.mjs"),
        Some("https://example.com/src/app/utils.mjs".to_string())
    );
}

#[test]
fn the_longest_matching_prefix_wins() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.set_import_map(
        "https://example.com/",
        r#"{"imports": {"@app/": "/generic/", "@app/special/": "/specific/"}}"#,
    )
    .expect("valid import map JSON should parse");
    assert_eq!(
        ctx.resolve_module_specifier("https://example.com/main.mjs", "@app/special/thing.mjs"),
        Some("https://example.com/specific/thing.mjs".to_string())
    );
}

#[test]
fn a_relative_specifier_never_consults_the_import_map() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    // Deliberately maps a key that would collide if relative specifiers
    // were ever routed through the map - real spec never does this.
    ctx.set_import_map(
        "https://example.com/",
        r#"{"imports": {"./sibling.mjs": "https://wrong.example.com/x.mjs"}}"#,
    )
    .expect("valid import map JSON should parse");
    assert_eq!(
        ctx.resolve_module_specifier("https://example.com/app/main.mjs", "./sibling.mjs"),
        Some("https://example.com/app/sibling.mjs".to_string())
    );
}

#[test]
fn set_import_map_rejects_malformed_json() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert!(ctx
        .set_import_map("https://example.com/", "not json")
        .is_err());
}

#[test]
fn an_import_map_with_no_imports_key_is_valid_and_contributes_nothing() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert!(ctx.set_import_map("https://example.com/", "{}").is_ok());
    assert_eq!(
        ctx.resolve_module_specifier("https://example.com/main.mjs", "lodash"),
        None
    );
}

#[test]
fn a_second_set_import_map_call_replaces_the_first_wholesale() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.set_import_map(
        "https://example.com/",
        r#"{"imports": {"a": "https://example.com/a.mjs"}}"#,
    )
    .unwrap();
    ctx.set_import_map(
        "https://example.com/",
        r#"{"imports": {"b": "https://example.com/b.mjs"}}"#,
    )
    .unwrap();
    assert_eq!(
        ctx.resolve_module_specifier("https://example.com/main.mjs", "a"),
        None,
        "the first map's entry should be gone after the second call"
    );
    assert_eq!(
        ctx.resolve_module_specifier("https://example.com/main.mjs", "b"),
        Some("https://example.com/b.mjs".to_string())
    );
}
