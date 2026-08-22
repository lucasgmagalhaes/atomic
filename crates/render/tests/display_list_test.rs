use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block, Color};
use render::build_display_list;

#[test]
fn transparent_box_produces_no_rect() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert!(build_display_list(&tree).is_empty());
}

#[test]
fn colored_box_produces_one_rect_matching_its_dimensions() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; height: 50px; background-color: red; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].width, 100.0);
    assert_eq!(list[0].height, 50.0);
    assert_eq!(list[0].color, Color { r: 255, g: 0, b: 0, a: 255 });
}

#[test]
fn parent_paints_before_children_so_children_can_draw_on_top() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let sheet = parse_stylesheet(
        "div { background-color: blue; } span { background-color: red; height: 10px; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].color, Color { r: 0, g: 0, b: 255, a: 255 });
    assert_eq!(list[1].color, Color { r: 255, g: 0, b: 0, a: 255 });
}

#[test]
fn only_colored_descendants_are_collected_among_transparent_ones() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let transparent_child = d.create_element("span");
    let colored_grandchild = d.create_element("em");
    d.append_child(root, parent);
    d.append_child(parent, transparent_child);
    d.append_child(transparent_child, colored_grandchild);

    let sheet = parse_stylesheet("em { background-color: #00ff00; height: 5px; }");
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].color, Color { r: 0, g: 255, b: 0, a: 255 });
}
