use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block, Color};

/// Builds `<p>hello <b>world</b></p>`'s DOM. Whether `<b>` actually flows
/// inline depends on the stylesheet passed to `build_box_tree` separately
/// (this crate has no built-in tag→display table - see `tree`'s module
/// doc), so callers pair this with `b { display: inline; }` or not.
fn hello_bold_world_dom() -> (Dom, dom::NodeId) {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let hello = d.create_text("hello ");
    let b = d.create_element("b");
    let world = d.create_text("world");
    d.append_child(root, p);
    d.append_child(p, hello);
    d.append_child(p, b);
    d.append_child(b, world);
    (d, p)
}

#[test]
fn text_and_inline_element_merge_into_one_inline_box() {
    let (d, p) = hello_bold_world_dom();
    let sheet = parse_stylesheet("b { display: inline; }");
    let tree = build_box_tree(&d, p, &sheet).unwrap();

    // "hello " + "<b>world</b>" collapse into exactly one synthetic
    // inline box, not two separate stacked children.
    assert_eq!(tree.children.len(), 1);
    assert!(tree.children[0].inline_spans.is_some());
    assert!(tree.children[0].text.is_none());
}

#[test]
fn merged_inline_box_contains_both_spans_text() {
    let (d, p) = hello_bold_world_dom();
    let sheet = parse_stylesheet("b { display: inline; }");
    let tree = build_box_tree(&d, p, &sheet).unwrap();

    let spans = tree.children[0].inline_spans.as_ref().unwrap();
    let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(joined, "hello world");
}

#[test]
fn inline_element_keeps_its_own_cascaded_color_as_a_distinct_span() {
    let (d, p) = hello_bold_world_dom();
    let sheet = parse_stylesheet("p { color: black; } b { display: inline; color: red; }");
    let mut tree = build_box_tree(&d, p, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let spans = tree.children[0].inline_spans.as_ref().unwrap();
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].text, "hello ");
    assert_eq!(
        spans[0].color,
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255
        }
    );
    assert_eq!(spans[1].text, "world");
    assert_eq!(
        spans[1].color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn glyphs_from_both_spans_end_up_on_the_same_shaped_box_with_correct_colors() {
    let (d, p) = hello_bold_world_dom();
    let sheet = parse_stylesheet("p { color: black; } b { display: inline; color: red; }");
    let mut tree = build_box_tree(&d, p, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let inline_box = &tree.children[0];
    assert!(!inline_box.glyphs.is_empty());

    let black = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    let red = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    assert!(
        inline_box.glyphs.iter().any(|g| g.color == black),
        "should have at least one black glyph from \"hello \""
    );
    assert!(
        inline_box.glyphs.iter().any(|g| g.color == red),
        "should have at least one red glyph from \"world\""
    );
}

#[test]
fn without_display_inline_the_element_lays_out_as_its_own_block_instead() {
    // No `display: inline` rule this time - `<b>` defaults to Block (this
    // crate has no UA stylesheet), so it stays a separate stacked box, not
    // merged with the surrounding text.
    let (d, p) = hello_bold_world_dom();
    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, p, &sheet).unwrap();

    assert_eq!(tree.children.len(), 2);
    assert_eq!(tree.children[0].text.as_deref(), Some("hello "));
    assert!(tree.children[0].inline_spans.is_none());
    assert!(tree.children[1].inline_spans.is_none());
    assert!(tree.children[1].text.is_none());
    assert_eq!(tree.children[1].children.len(), 1);
}

#[test]
fn a_lone_plain_text_child_still_uses_the_simple_text_box_not_inline_spans() {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("just text, no siblings");
    d.append_child(root, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, p, &sheet).unwrap();

    assert_eq!(tree.children.len(), 1);
    assert_eq!(
        tree.children[0].text.as_deref(),
        Some("just text, no siblings")
    );
    assert!(tree.children[0].inline_spans.is_none());
}

#[test]
fn nested_inline_elements_flatten_into_the_same_run() {
    // <p>a <b>b <i>c</i> d</b> e</p>, all inline.
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let a = d.create_text("a ");
    let b = d.create_element("b");
    let b_text1 = d.create_text("b ");
    let i = d.create_element("i");
    let i_text = d.create_text("c");
    let b_text2 = d.create_text(" d");
    let e = d.create_text(" e");

    d.append_child(root, p);
    d.append_child(p, a);
    d.append_child(p, b);
    d.append_child(b, b_text1);
    d.append_child(b, i);
    d.append_child(i, i_text);
    d.append_child(b, b_text2);
    d.append_child(p, e);

    let sheet = parse_stylesheet("b, i { display: inline; }");
    let tree = build_box_tree(&d, p, &sheet).unwrap();

    assert_eq!(
        tree.children.len(),
        1,
        "the whole thing should be one merged inline run"
    );
    let spans = tree.children[0].inline_spans.as_ref().unwrap();
    let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(joined, "a b c d e");
}
