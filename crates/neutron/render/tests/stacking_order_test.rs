use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};
use render::build_display_list;

#[test]
fn negative_z_index_paints_behind_a_later_non_positioned_sibling() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let normal = d.create_element("span");
    let behind = d.create_element("em");
    d.append_child(root, parent);
    d.append_child(parent, normal);
    d.append_child(parent, behind);

    let sheet = parse_stylesheet(
        "span { width: 10px; height: 10px; background-color: red; } \
         em { width: 10px; height: 10px; background-color: blue; position: relative; z-index: -1; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // `em` comes second in tree order but has a negative z-index, so it
    // must paint first (furthest back) despite that.
    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].color.b, 255);
    assert_eq!(list[1].color.r, 255);
}

#[test]
fn positive_z_index_paints_in_front_of_an_earlier_non_positioned_sibling() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let front = d.create_element("em");
    let normal = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, front);
    d.append_child(parent, normal);

    let sheet = parse_stylesheet(
        "em { width: 10px; height: 10px; background-color: blue; position: relative; z-index: 1; } \
         span { width: 10px; height: 10px; background-color: red; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // `em` comes first in tree order but has a positive z-index, so it
    // must paint last (on top) despite that.
    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].color.r, 255);
    assert_eq!(list[1].color.b, 255);
}

#[test]
fn equal_z_index_siblings_keep_their_tree_order() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let first = d.create_element("em");
    let second = d.create_element("i");
    d.append_child(root, parent);
    d.append_child(parent, first);
    d.append_child(parent, second);

    let sheet = parse_stylesheet(
        "em { width: 10px; height: 10px; background-color: red; position: relative; z-index: 1; } \
         i { width: 10px; height: 10px; background-color: blue; position: relative; z-index: 1; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].color.r, 255);
    assert_eq!(list[1].color.b, 255);
}

#[test]
fn z_index_without_position_never_establishes_a_stacking_context() {
    // `z-index` alone (no `position`) is a documented no-op per the real
    // spec's own trigger (`position != static` *and* an explicit
    // z-index) - a `static` box with z-index must paint in plain tree
    // order, same as if it had none at all.
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let first = d.create_element("em");
    let second = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, first);
    d.append_child(parent, second);

    let sheet = parse_stylesheet(
        "em { width: 10px; height: 10px; background-color: red; z-index: 5; } \
         span { width: 10px; height: 10px; background-color: blue; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].color.r, 255);
    assert_eq!(list[1].color.b, 255);
}
