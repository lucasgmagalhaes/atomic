use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};
use render::build_display_list;

#[test]
fn a_sticky_box_with_a_top_offset_is_tagged_with_its_natural_y_and_top_px() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet(
        "div { position: sticky; top: 10px; width: 50px; height: 20px; background-color: red; }",
    );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    // A sticky box stays in normal flow (unlike fixed/absolute) - its
    // dimensions.y is its real, unstuck document position.
    assert_eq!(list[0].y, 0.0);
    assert_eq!(list[0].sticky, Some((0.0, 10.0)));
    assert!(!list[0].fixed);
}

#[test]
fn sticky_with_no_top_offset_is_not_tagged_sticky_at_all() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet(
        "div { position: sticky; width: 50px; height: 20px; background-color: red; }",
    );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].sticky, None,
        "no usable `top` means this box just behaves like static/relative"
    );
}

#[test]
fn stickiness_propagates_to_every_descendant_with_the_sticky_ancestors_own_values() {
    let mut d = Dom::new();
    let root = d.root();
    let header = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(header, child);
    d.append_child(root, header);

    let sheet = parse_stylesheet(
        "div { position: sticky; top: 0px; background-color: blue; } \
         span { display: block; background-color: green; }",
    );
    let mut tree = build_box_tree(&d, header, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    let expected = Some((0.0, 0.0));
    assert!(
        list.iter().all(|r| r.sticky == expected),
        "every rect under a sticky ancestor should share its (natural_y, top_px), got {list:?}"
    );
}
