use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};
use render::{build_display_list, build_glyph_list, build_image_list};

#[test]
fn a_fixed_box_own_rect_is_tagged_fixed() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet(
        "div { position: fixed; width: 50px; height: 50px; background-color: red; }",
    );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert!(
        list[0].fixed,
        "a fixed box's own rect should be tagged fixed"
    );
}

#[test]
fn a_normal_box_rect_is_not_tagged_fixed() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 50px; height: 50px; background-color: red; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert!(!list[0].fixed);
}

#[test]
fn fixed_ness_propagates_to_every_descendant_rect() {
    let mut d = Dom::new();
    let root = d.root();
    let header = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(header, child);
    d.append_child(root, header);

    let sheet = parse_stylesheet(
        "div { position: fixed; background-color: blue; } \
         span { display: block; background-color: green; }",
    );
    let mut tree = build_box_tree(&d, header, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(
        list.len(),
        2,
        "both the fixed div and its child span should paint"
    );
    assert!(
        list.iter().all(|r| r.fixed),
        "every rect under a fixed ancestor should itself be tagged fixed, got {list:?}"
    );
}

#[test]
fn fixed_ness_propagates_to_images_and_glyphs_too() {
    let mut d = Dom::new();
    let root = d.root();
    let header = d.create_element("div");
    let text = d.create_text("pinned");
    d.append_child(header, text);
    d.append_child(root, header);

    let sheet = parse_stylesheet("div { position: fixed; }");
    let mut tree = build_box_tree(&d, header, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let glyphs = build_glyph_list(&tree);
    assert!(!glyphs.is_empty());
    assert!(glyphs.iter().all(|g| g.fixed));

    // No <img> in this tree, but build_image_list should still accept and
    // return an empty (not panic) list - a plain sanity check that the
    // new `fixed` field didn't break this builder's own empty-input path.
    assert!(build_image_list(&tree).is_empty());
}
