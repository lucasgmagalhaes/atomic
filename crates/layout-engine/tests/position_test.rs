use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};

#[test]
fn relative_position_shifts_the_box_without_moving_the_next_sibling() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let a = d.create_element("div");
    let b = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, a);
    d.append_child(container, b);
    d.set_attribute(a, "id", "a");

    let sheet = parse_stylesheet("div { height: 30px; } #a { position: relative; top: 10px; left: 5px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // `a` is visually shifted by its own top/left...
    assert_eq!(tree.children[0].dimensions.x, 5.0);
    assert_eq!(tree.children[0].dimensions.y, 10.0);
    // ...but `b` starts exactly where it would if `a` had never been
    // shifted (real CSS: a relative offset is purely visual, the space
    // `a` occupies in the flow is untouched).
    assert_eq!(tree.children[1].dimensions.x, 0.0);
    assert_eq!(tree.children[1].dimensions.y, 30.0);
}

#[test]
fn relative_position_offset_carries_through_to_descendants() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let sheet = parse_stylesheet("div { position: relative; top: 20px; left: 15px; }");
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.dimensions.x, 15.0);
    assert_eq!(tree.dimensions.y, 20.0);
    // The child was laid out against the parent's already-shifted content
    // origin, not the parent's pre-shift position.
    assert_eq!(tree.children[0].dimensions.x, 15.0);
    assert_eq!(tree.children[0].dimensions.y, 20.0);
}

#[test]
fn relative_position_falls_back_to_right_and_bottom_when_top_left_are_auto() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { position: relative; right: 10px; bottom: 4px; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // `right`/`bottom` push the box in the negative direction, real CSS's
    // own convention (only consulted because `left`/`top` are `Auto`).
    assert_eq!(tree.dimensions.x, -10.0);
    assert_eq!(tree.dimensions.y, -4.0);
}

#[test]
fn absolute_position_removes_the_box_from_flow() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let a = d.create_element("div");
    let b = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, a);
    d.append_child(container, b);
    d.set_attribute(a, "id", "a");

    let sheet = parse_stylesheet("div { height: 30px; } #a { position: absolute; top: 100px; left: 50px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // `a` is positioned independently, not in normal flow.
    assert_eq!(tree.children[0].dimensions.x, 50.0);
    assert_eq!(tree.children[0].dimensions.y, 100.0);
    // `b` starts as if `a` were never there at all (real CSS: an
    // absolutely positioned box doesn't occupy space in its parent's flow).
    assert_eq!(tree.children[1].dimensions.y, 0.0);
    // The parent's auto height also ignores `a` entirely.
    assert_eq!(tree.dimensions.height, 30.0);
}

#[test]
fn absolute_position_with_no_offsets_lands_at_the_page_origin() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let sheet = parse_stylesheet("div { padding: 40px; } span { position: absolute; }");
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // No `top`/`left`/`right`/`bottom` at all: this crate's real, narrower-
    // than-spec scope resolves it against the page's own origin, not the
    // parent's content box (which real "static position" fallback would
    // use) - so it lands at (0, 0), not (40, 40) inside the padded parent.
    assert_eq!(tree.children[0].dimensions.x, 0.0);
    assert_eq!(tree.children[0].dimensions.y, 0.0);
}

#[test]
fn absolute_position_still_resolves_its_own_width_against_the_parent_content_width() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let sheet = parse_stylesheet("div { width: 300px; padding: 20px; } span { position: absolute; }");
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // `width: auto` on the absolutely positioned child still fills the
    // parent's own content width (300px), even though its *position* is
    // independent of the parent - width/height resolve against the
    // nearest layout ancestor, only x/y skip it (see `offset_from_edges`'s
    // own doc for the exact scope).
    assert_eq!(tree.children[0].dimensions.width, 300.0);
}
