use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};

fn build(html_css: &str, children_tags: &[&str]) -> (Dom, dom::NodeId) {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    d.append_child(root, container);
    for tag in children_tags {
        let child = d.create_element(tag);
        d.append_child(container, child);
    }
    let _ = html_css;
    (d, container)
}

#[test]
fn row_flex_places_children_side_by_side() {
    let (d, container) = build("", &["span", "span"]);
    let sheet = parse_stylesheet(
        "div { display: flex; } span { flex-basis: 50px; flex-grow: 0; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.x, 0.0);
    assert_eq!(tree.children[0].dimensions.width, 50.0);
    assert_eq!(tree.children[1].dimensions.x, 50.0);
    assert_eq!(tree.children[1].dimensions.y, 0.0);
}

#[test]
fn flex_grow_distributes_free_space_by_weight() {
    let (d, container) = build("", &["span", "span"]);
    let sheet = parse_stylesheet("div { display: flex; } span { flex-basis: 0px; flex-grow: 1; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // 800px free space split evenly between two grow:1 items.
    assert_eq!(tree.children[0].dimensions.width, 400.0);
    assert_eq!(tree.children[1].dimensions.width, 400.0);
    assert_eq!(tree.children[1].dimensions.x, 400.0);
}

#[test]
fn flex_grow_weighted_unevenly() {
    let (d, container) = build("", &["span", "p"]);
    let sheet = parse_stylesheet(
        "div { display: flex; } \
         span { flex-basis: 0px; flex-grow: 1; } \
         p { flex-basis: 0px; flex-grow: 3; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.width, 200.0);
    assert_eq!(tree.children[1].dimensions.width, 600.0);
}

#[test]
fn flex_shrink_reduces_items_below_basis_when_overflowing() {
    let (d, container) = build("", &["span", "p"]);
    let sheet = parse_stylesheet(
        "div { display: flex; width: 100px; } \
         span { flex-basis: 100px; flex-shrink: 1; } \
         p { flex-basis: 100px; flex-shrink: 1; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // total basis 200 in a 100px container, equal shrink weight -> 50/50.
    assert_eq!(tree.children[0].dimensions.width, 50.0);
    assert_eq!(tree.children[1].dimensions.width, 50.0);
}

#[test]
fn justify_content_center_centers_items_with_leftover_space() {
    let (d, container) = build("", &["span"]);
    let sheet =
        parse_stylesheet("div { display: flex; justify-content: center; } span { flex-basis: 100px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // 700px leftover / 2 = 350px offset.
    assert_eq!(tree.children[0].dimensions.x, 350.0);
}

#[test]
fn justify_content_space_between_spreads_items() {
    let (d, container) = build("", &["span", "p", "b"]);
    let sheet = parse_stylesheet(
        "div { display: flex; justify-content: space-between; } \
         span, p, b { flex-basis: 100px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // 500px leftover / 2 gaps = 250px gap between each 100px item.
    assert_eq!(tree.children[0].dimensions.x, 0.0);
    assert_eq!(tree.children[1].dimensions.x, 350.0);
    assert_eq!(tree.children[2].dimensions.x, 700.0);
}

#[test]
fn align_items_stretch_fills_container_height_by_default() {
    let (d, container) = build("", &["span"]);
    let sheet = parse_stylesheet("div { display: flex; height: 200px; } span { flex-basis: 50px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.height, 200.0);
}

#[test]
fn align_items_center_centers_on_cross_axis() {
    let (d, container) = build("", &["span"]);
    let sheet = parse_stylesheet(
        "div { display: flex; height: 200px; align-items: center; } \
         span { flex-basis: 50px; height: 40px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.height, 40.0);
    assert_eq!(tree.children[0].dimensions.y, 80.0); // (200-40)/2
}

#[test]
fn column_direction_stacks_children_vertically_by_height() {
    let (d, container) = build("", &["span", "p"]);
    let sheet = parse_stylesheet(
        "div { display: flex; flex-direction: column; } \
         span { flex-basis: 30px; } p { flex-basis: 40px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[0].dimensions.height, 30.0);
    assert_eq!(tree.children[1].dimensions.y, 30.0);
    assert_eq!(tree.children[1].dimensions.height, 40.0);
    // Column container's auto height = sum of children's main sizes.
    assert_eq!(tree.dimensions.height, 70.0);
}

#[test]
fn column_direction_cross_axis_is_width() {
    let (d, container) = build("", &["span"]);
    let sheet = parse_stylesheet("div { display: flex; flex-direction: column; width: 300px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // Stretch (default align-items) fills the 300px width.
    assert_eq!(tree.children[0].dimensions.width, 300.0);
}

#[test]
fn nested_flex_containers_lay_out_grandchildren() {
    let mut d = Dom::new();
    let root = d.root();
    let outer = d.create_element("div");
    let inner = d.create_element("section");
    let leaf = d.create_element("span");
    d.append_child(root, outer);
    d.append_child(outer, inner);
    d.append_child(inner, leaf);

    let sheet = parse_stylesheet(
        "div { display: flex; } \
         section { flex-basis: 200px; display: flex; } \
         span { flex-basis: 60px; }",
    );
    let mut tree = build_box_tree(&d, outer, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let inner_box = &tree.children[0];
    assert_eq!(inner_box.dimensions.width, 200.0);
    let leaf_box = &inner_box.children[0];
    assert_eq!(leaf_box.dimensions.width, 60.0);
    // leaf's x is relative to inner's content box, which starts at
    // inner's own x (0, since flex items start at container origin).
    assert_eq!(leaf_box.dimensions.x, 0.0);
}
