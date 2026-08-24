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

#[test]
fn bordered_box_produces_four_real_border_strips_around_its_edge() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; height: 50px; border: 5px solid blue; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // No background here - just the 4 border strips. `dimensions` is the
    // real border box (100 + 5 + 5 wide, 50 + 5 + 5 tall - see
    // `layout_engine::layout::layout_block`'s own doc on real box-model
    // growth from `border-width`).
    assert_eq!(tree.dimensions.width, 110.0);
    assert_eq!(tree.dimensions.height, 60.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 4);
    for rect in &list {
        assert_eq!(rect.color, Color { r: 0, g: 0, b: 255, a: 255 });
    }
    // top
    assert!(list.iter().any(|r| r.x == 0.0 && r.y == 0.0 && r.width == 110.0 && r.height == 5.0));
    // bottom
    assert!(list.iter().any(|r| r.x == 0.0 && r.y == 55.0 && r.width == 110.0 && r.height == 5.0));
    // left
    assert!(list.iter().any(|r| r.x == 0.0 && r.y == 0.0 && r.width == 5.0 && r.height == 60.0));
    // right
    assert!(list.iter().any(|r| r.x == 105.0 && r.y == 0.0 && r.width == 5.0 && r.height == 60.0));
}

#[test]
fn border_style_none_produces_no_border_rects_even_with_a_width_set() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; height: 50px; border-width: 5px; border-style: none; border-color: red; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert!(build_display_list(&tree).is_empty());
    // Real spec rule: no style means the width contributes nothing to the
    // box model either, not just to painting.
    assert_eq!(tree.dimensions.width, 100.0);
}

#[test]
fn zero_width_border_side_contributes_no_rect() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; height: 50px; border-style: solid; border-color: green; border-width: 0px 3px 0px 3px; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    // Only left/right (3px each) - top/bottom are 0px and contribute
    // nothing, same as a transparent background contributing no rect.
    assert_eq!(list.len(), 2);
}

#[test]
fn background_and_border_paint_in_the_right_order_for_a_bordered_colored_box() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; height: 50px; background-color: #ffff00; border: 2px solid black; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    // Background first (so the border strips paint on top of it, real
    // paint order), then the 4 border strips.
    assert_eq!(list.len(), 5);
    assert_eq!(list[0].color, Color { r: 255, g: 255, b: 0, a: 255 });
    for rect in &list[1..] {
        assert_eq!(rect.color, Color { r: 0, g: 0, b: 0, a: 255 });
    }
}
