use css::{matching_declarations, parse_stylesheet, ElementSnapshot};
use dom::Dom;
use layout_engine::{build_box_tree, resolve_style, Color, FontFamily, GenericFontFamily};

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
fn font_family_is_unset_by_default_at_the_cascade_level() {
    let sheet = parse_stylesheet("div { color: red; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    // `resolve_style` alone never fills in an inherited default - see
    // `ComputedStyle::font_family`'s own doc. Real inheritance happens one
    // layer up, in the box-tree build chain (tested below via
    // `build_box_tree`).
    assert!(style.font_family.is_none());
}

#[test]
fn font_family_with_a_named_font_and_generic_fallback() {
    let sheet = parse_stylesheet("div { font-family: Arial, sans-serif; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    let f = style.font_family.expect("expected a resolved font-family");
    assert_eq!(f.name(), Some("Arial"));
    assert_eq!(f.generic, GenericFontFamily::SansSerif);
}

#[test]
fn font_family_with_only_a_generic_keyword() {
    let sheet = parse_stylesheet("div { font-family: monospace; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    let f = style.font_family.expect("expected a resolved font-family");
    assert_eq!(f.name(), None);
    assert_eq!(f.generic, GenericFontFamily::Monospace);
}

#[test]
fn a_quoted_font_name_is_recognized_too() {
    let sheet = parse_stylesheet("div { font-family: \"Helvetica Neue\", sans-serif; }");
    let style = resolve_style(
        &matching_declarations(&sheet, &[el("div")], 1024.0, 768.0),
        16.0,
        BLACK,
    );
    let f = style.font_family.expect("expected a resolved font-family");
    assert_eq!(f.name(), Some("Helvetica Neue"));
}

#[test]
fn real_inheritance_reaches_a_grandchild_with_no_font_family_rule_of_its_own() {
    let mut d = Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    let div = d.create_element("div");
    let span = d.create_element("span");
    let text = d.create_text("hi");
    d.append_child(span, text);
    d.append_child(div, span);
    d.append_child(body, div);
    d.append_child(root, body);

    let sheet = parse_stylesheet("body { font-family: Georgia, serif; }");
    let tree = build_box_tree(&d, body, &sheet).unwrap();

    // tree = body, tree.children[0] = div, its child = span (both display:
    // block by default, so span isn't merged into an inline run) - span's
    // own resolved style should carry the inherited family two levels
    // down, with no rule of its own setting it.
    let div_box = &tree.children[0];
    let span_box = &div_box.children[0];
    let f = span_box
        .style
        .font_family
        .expect("inherited font-family should reach the grandchild");
    assert_eq!(f.name(), Some("Georgia"));
    assert_eq!(f.generic, GenericFontFamily::Serif);
}

#[test]
fn a_descendant_rule_overrides_the_inherited_family() {
    let mut d = Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    let code = d.create_element("code");
    d.append_child(body, code);
    d.append_child(root, body);

    let sheet =
        parse_stylesheet("body { font-family: Georgia, serif; } code { font-family: monospace; }");
    let tree = build_box_tree(&d, body, &sheet).unwrap();

    let code_box = &tree.children[0];
    let f = code_box.style.font_family.unwrap();
    assert_eq!(f.generic, GenericFontFamily::Monospace);
    assert_eq!(f.name(), None);
}

#[test]
fn with_no_font_family_rule_anywhere_the_root_falls_back_to_the_built_in_default() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, div, &sheet).unwrap();

    assert_eq!(tree.style.font_family, Some(FontFamily::default()));
    assert_eq!(FontFamily::default().generic, GenericFontFamily::SansSerif);
}

#[test]
fn register_font_face_never_panics_on_unparseable_bytes() {
    // Real `@font-face` (`page_source::load_font_faces`'s one real
    // consumer): `fontdb`'s own loader just logs a warning and registers
    // zero faces for data it can't parse - never panics - matching this
    // crate's "best-effort, never fail the whole page load over one bad
    // resource" stance for every other optional network resource.
    layout_engine::register_font_face(b"not a real font file".to_vec());
    layout_engine::register_font_face(Vec::new());

    // Layout/shaping still works normally afterwards - registering garbage
    // data doesn't poison the shared font system for anything else.
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);
    let sheet = parse_stylesheet("div { font-family: \"DefinitelyNotRegistered\", sans-serif; }");
    let tree = build_box_tree(&d, div, &sheet).unwrap();
    assert_eq!(
        tree.style.font_family.unwrap().name(),
        Some("DefinitelyNotRegistered")
    );
}
