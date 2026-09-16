use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block, Color};
use render::build_display_list;

#[test]
fn box_shadow_paints_one_offset_rect_behind_the_box() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet(
        "div { width: 50px; height: 50px; background-color: #ffff00; box-shadow: 5px 10px blue; }",
    );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    // Shadow first (paints behind, per real box-shadow stacking), then
    // the box's own background on top of it.
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].x, 5.0);
    assert_eq!(list[0].y, 10.0);
    assert_eq!(list[0].width, 50.0);
    assert_eq!(list[0].height, 50.0);
    assert_eq!(
        list[0].color,
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }
    );
    assert_eq!(
        list[1].color,
        Color {
            r: 255,
            g: 255,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn box_shadow_spread_grows_the_shadow_rect_on_every_side() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet =
        parse_stylesheet("div { width: 50px; height: 50px; box-shadow: 0px 0px 0px 5px red; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].x, -5.0);
    assert_eq!(list[0].y, -5.0);
    assert_eq!(list[0].width, 60.0);
    assert_eq!(list[0].height, 60.0);
}

#[test]
fn box_shadow_none_produces_no_shadow_rect() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 50px; height: 50px; box-shadow: none; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert!(build_display_list(&tree).is_empty());
}

#[test]
fn box_shadow_is_clipped_by_an_overflow_hidden_ancestor() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let sheet = parse_stylesheet(
        "div { width: 20px; height: 20px; overflow: hidden; } \
         span { width: 20px; height: 20px; box-shadow: 100px 100px red; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // The shadow is offset 100px past the 20x20 clip region - entirely
    // clipped away, same as any other paint primitive under an
    // overflow: hidden ancestor.
    assert!(build_display_list(&tree).is_empty());
}
