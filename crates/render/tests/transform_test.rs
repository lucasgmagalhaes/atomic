use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};
use render::build_display_list;

#[test]
fn transform_translate_offsets_a_boxs_own_painted_rect() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet(
        "div { width: 10px; height: 10px; background-color: red; transform: translate(30px, 40px); }",
    );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].x, 30.0);
    assert_eq!(list[0].y, 40.0);
}

#[test]
fn transform_moves_the_whole_subtree_as_one_rigid_unit() {
    let mut d = Dom::new();
    let root = d.root();
    let outer = d.create_element("div");
    let inner = d.create_element("span");
    d.append_child(root, outer);
    d.append_child(outer, inner);

    let sheet = parse_stylesheet(
        "div { transform: translate(100px, 0px); } \
         span { width: 5px; height: 5px; background-color: blue; }",
    );
    let mut tree = build_box_tree(&d, outer, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    // `span` itself has no transform, but its transformed ancestor's
    // offset must still carry down to it - the whole subtree moves.
    assert_eq!(
        list[0].x, 100.0,
        "an untransformed child must still move with its transformed ancestor"
    );
}

#[test]
fn transform_does_not_affect_sibling_layout_position() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let moved = d.create_element("span");
    let sibling = d.create_element("em");
    d.append_child(root, parent);
    d.append_child(parent, moved);
    d.append_child(parent, sibling);

    let sheet = parse_stylesheet(
        "span { width: 10px; height: 10px; background-color: red; transform: translate(50px, 0px); } \
         em { width: 10px; height: 10px; background-color: blue; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    // `span` is a block box 10px tall, so `em` lays out at y=10 as if
    // `span` were never transformed - transform is paint-only, per
    // `layout_engine::ComputedStyle::transform`'s own doc.
    assert_eq!(list[1].y, 10.0);
    assert_eq!(
        list[1].x, 0.0,
        "an untransformed sibling's own layout position must be unaffected by a transformed sibling"
    );
}
