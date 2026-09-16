use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};

#[test]
fn auto_width_fills_containing_block() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.dimensions.x, 0.0);
    assert_eq!(tree.dimensions.y, 0.0);
    assert_eq!(tree.dimensions.width, 800.0);
}

#[test]
fn explicit_px_width_is_used_directly() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 200px; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.dimensions.width, 200.0);
}

#[test]
fn margin_offsets_position_and_shrinks_auto_width() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { margin: 10px; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.dimensions.x, 10.0);
    assert_eq!(tree.dimensions.y, 10.0);
    // auto width fills remaining space after both side margins.
    assert_eq!(tree.dimensions.width, 780.0);
}

#[test]
fn padding_adds_to_the_painted_box_on_top_of_content_width() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; padding: 10px; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // dimensions.width is the padding box: 100 content + 10 + 10 padding.
    assert_eq!(tree.dimensions.width, 120.0);
}

#[test]
fn children_stack_vertically_and_parent_height_is_auto() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child1 = d.create_element("p");
    let child2 = d.create_element("p");
    d.append_child(root, parent);
    d.append_child(parent, child1);
    d.append_child(parent, child2);

    let sheet = parse_stylesheet("p { height: 30px; }");
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[0].dimensions.height, 30.0);
    assert_eq!(tree.children[1].dimensions.y, 30.0);
    // Parent's auto height sums its children's total (margin-box) heights.
    assert_eq!(tree.dimensions.height, 60.0);
}

#[test]
fn child_content_width_is_parents_content_width_not_padding_box() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let sheet = parse_stylesheet("div { width: 100px; padding: 10px; }");
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // Child's auto width fills the parent's *content* width (100px),
    // not the parent's padding-box width (120px).
    assert_eq!(tree.children[0].dimensions.width, 100.0);
    assert_eq!(tree.children[0].dimensions.x, 10.0); // parent's padding-left
}

#[test]
fn explicit_height_overrides_auto_content_height() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { height: 500px; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.dimensions.height, 500.0);
}
