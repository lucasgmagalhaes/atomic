use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};

#[test]
fn a_left_float_sits_flush_to_the_containing_blocks_left_edge() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let float = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, float);

    let sheet = parse_stylesheet("div { width: 200px; } div div { float: left; width: 50px; height: 40px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.x, 0.0);
    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[0].dimensions.width, 50.0);
}

#[test]
fn a_right_float_sits_flush_to_the_containing_blocks_right_edge() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let float = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, float);

    let sheet = parse_stylesheet("div { width: 200px; } div div { float: right; width: 50px; height: 40px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // Container is 200px wide, float is 50px wide - flush right means
    // x = 200 - 50 = 150.
    assert_eq!(tree.children[0].dimensions.x, 150.0);
    assert_eq!(tree.children[0].dimensions.y, 0.0);
}

#[test]
fn left_and_right_floats_can_sit_side_by_side_at_the_same_height() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let left = d.create_element("div");
    let right = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, left);
    d.append_child(container, right);
    d.set_attribute(left, "id", "left");
    d.set_attribute(right, "id", "right");

    let sheet = parse_stylesheet(
        "div { width: 200px; } \
         #left { float: left; width: 50px; height: 40px; } \
         #right { float: right; width: 30px; height: 20px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.x, 0.0);
    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[1].dimensions.x, 170.0);
    assert_eq!(tree.children[1].dimensions.y, 0.0);
}

#[test]
fn a_second_same_side_float_stacks_below_the_first_not_beside_it() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let first = d.create_element("div");
    let second = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, first);
    d.append_child(container, second);

    let sheet = parse_stylesheet("div { width: 200px; } div div { float: left; width: 50px; height: 40px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.y, 0.0);
    // Second left float is placed below the first's bottom (40px), not
    // beside it - the real, documented scope cut (see
    // `layout::layout_children`'s own doc: no float-row packing).
    assert_eq!(tree.children[1].dimensions.x, 0.0);
    assert_eq!(tree.children[1].dimensions.y, 40.0);
}

#[test]
fn a_float_does_not_push_down_the_next_normal_sibling() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let float = d.create_element("div");
    let normal = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, float);
    d.append_child(container, normal);
    d.set_attribute(float, "id", "float");

    let sheet = parse_stylesheet("div { width: 200px; height: 10px; } #float { float: left; width: 50px; height: 100px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // The float is 100px tall, but the following normal sibling still
    // starts at y=0 - real CSS: a float doesn't advance the normal flow
    // cursor for later siblings (only `clear` does that explicitly).
    assert_eq!(tree.children[1].dimensions.y, 0.0);
}

#[test]
fn clear_pushes_a_box_below_every_earlier_same_side_float() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let float = d.create_element("div");
    let cleared = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, float);
    d.append_child(container, cleared);
    d.set_attribute(float, "id", "float");
    d.set_attribute(cleared, "id", "cleared");

    let sheet = parse_stylesheet(
        "div { width: 200px; } \
         #float { float: left; width: 50px; height: 100px; } \
         #cleared { clear: left; height: 10px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[1].dimensions.y, 100.0);
}

#[test]
fn clear_both_clears_whichever_side_is_lower() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let left = d.create_element("div");
    let right = d.create_element("div");
    let cleared = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, left);
    d.append_child(container, right);
    d.append_child(container, cleared);
    d.set_attribute(left, "id", "left");
    d.set_attribute(right, "id", "right");
    d.set_attribute(cleared, "id", "cleared");

    let sheet = parse_stylesheet(
        "div { width: 200px; } \
         #left { float: left; width: 50px; height: 30px; } \
         #right { float: right; width: 30px; height: 80px; } \
         #cleared { clear: both; height: 10px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // Right float is taller (80px) than left (30px) - `clear: both` must
    // clear the lower of the two.
    assert_eq!(tree.children[2].dimensions.y, 80.0);
}

#[test]
fn a_non_cleared_sibling_after_a_float_can_overlap_it_visually() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let float = d.create_element("div");
    let normal = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, float);
    d.append_child(container, normal);
    d.set_attribute(float, "id", "float");

    let sheet = parse_stylesheet("div { width: 200px; } #float { float: left; width: 50px; height: 100px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // No line-box narrowing is modeled (see `layout::layout_children`'s
    // own doc) - a normal sibling spans the container's full width,
    // genuinely overlapping the float's own space rather than wrapping
    // around it.
    assert_eq!(tree.children[1].dimensions.x, 0.0);
    assert_eq!(tree.children[1].dimensions.width, 200.0);
}

#[test]
fn a_container_grows_its_own_height_to_contain_its_floats() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let float = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, float);

    let sheet = parse_stylesheet("div { width: 200px; } div div { float: left; width: 50px; height: 100px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // Deliberate simplification (see `layout::layout_children`'s own
    // doc): this engine's block containers always grow to contain their
    // floats, unlike real CSS's default (only a container establishing
    // its own block formatting context does).
    assert_eq!(tree.dimensions.height, 100.0);
}

#[test]
fn float_none_is_the_default_and_behaves_like_a_normal_block() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let a = d.create_element("div");
    let b = d.create_element("div");
    d.append_child(root, container);
    d.append_child(container, a);
    d.append_child(container, b);

    let sheet = parse_stylesheet("div { width: 200px; height: 20px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[1].dimensions.y, 20.0);
}
