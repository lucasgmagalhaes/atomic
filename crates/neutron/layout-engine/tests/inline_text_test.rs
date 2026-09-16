use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};

#[test]
fn text_box_is_positioned_and_sized_by_its_shaped_content() {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("hello world");
    d.append_child(root, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, p, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let text_box = &tree.children[0];
    assert!(text_box.dimensions.width > 0.0);
    assert!(text_box.dimensions.height > 0.0);
    assert!(!text_box.glyphs.is_empty());
    // The parent's auto height comes from the text box's measured height.
    assert_eq!(tree.dimensions.height, text_box.dimensions.height);
}

#[test]
fn text_wraps_within_a_narrow_containing_block() {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("hello world this is a long sentence that should wrap");
    d.append_child(root, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("p { width: 100px; }");
    let mut tree = build_box_tree(&d, p, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let text_box = &tree.children[0];
    assert!(text_box.dimensions.width <= 100.0 + 1.0);

    // Compare against the same text laid out with plenty of room - wrapping
    // to more lines should make the wrapped version measurably taller.
    let mut d2 = Dom::new();
    let root2 = d2.root();
    let p2 = d2.create_element("p");
    let text2 = d2.create_text("hello world this is a long sentence that should wrap");
    d2.append_child(root2, p2);
    d2.append_child(p2, text2);
    let sheet2 = parse_stylesheet("");
    let mut wide_tree = build_box_tree(&d2, p2, &sheet2).unwrap();
    layout_block(&mut wide_tree, 800.0, 0.0, 0.0);

    assert!(text_box.dimensions.height > wide_tree.children[0].dimensions.height);
}

#[test]
fn font_size_is_inherited_from_ancestor_elements() {
    let mut d = Dom::new();
    let root = d.root();
    let section = d.create_element("section");
    let p = d.create_element("p");
    let text = d.create_text("big text");
    d.append_child(root, section);
    d.append_child(section, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("section { font-size: 40px; }");
    let mut tree = build_box_tree(&d, section, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let text_box = &tree.children[0].children[0];
    assert_eq!(text_box.style.font_size, 40.0);

    // Same text at the default 16px should measure narrower.
    let mut d2 = Dom::new();
    let root2 = d2.root();
    let p2 = d2.create_element("p");
    let text2 = d2.create_text("big text");
    d2.append_child(root2, p2);
    d2.append_child(p2, text2);
    let sheet2 = parse_stylesheet("");
    let mut default_tree = build_box_tree(&d2, p2, &sheet2).unwrap();
    layout_block(&mut default_tree, 800.0, 0.0, 0.0);

    assert!(text_box.dimensions.width > default_tree.children[0].dimensions.width);
}

#[test]
fn color_is_inherited_and_reaches_glyphs() {
    use layout_engine::Color;

    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("x");
    d.append_child(root, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("p { color: #ff0000; }");
    let mut tree = build_box_tree(&d, p, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let text_box = &tree.children[0];
    assert_eq!(
        text_box.style.color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
    assert_eq!(
        text_box.glyphs[0].color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}
