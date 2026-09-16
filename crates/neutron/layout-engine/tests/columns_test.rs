//! CSS multicolumn layout — real `column-count`, splitting a block
//! container's children across N equal-width side-by-side columns. See
//! `crate::columns`'s own doc for the scope cut (count-based chunking,
//! not real height balancing).

use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};

fn build_children(count: usize) -> (Dom, dom::NodeId) {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    d.set_attribute(container, "id", "container");
    d.append_child(root, container);
    for _ in 0..count {
        let child = d.create_element("p");
        d.append_child(container, child);
    }
    (d, container)
}

#[test]
fn two_columns_split_four_children_evenly() {
    let (d, container) = build_children(4);
    let sheet = parse_stylesheet(
        "#container { column-count: 2; width: 200px; } \
         p { height: 10px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // 4 children / 2 columns = 2 per column.
    assert_eq!(tree.children[0].dimensions.x, 0.0);
    assert_eq!(tree.children[0].dimensions.width, 100.0);
    assert_eq!(tree.children[1].dimensions.x, 0.0);
    assert_eq!(tree.children[1].dimensions.y, 10.0);

    assert_eq!(tree.children[2].dimensions.x, 100.0);
    assert_eq!(tree.children[2].dimensions.y, 0.0);
    assert_eq!(tree.children[3].dimensions.x, 100.0);
    assert_eq!(tree.children[3].dimensions.y, 10.0);
}

#[test]
fn column_width_is_content_width_divided_by_column_count() {
    let (d, container) = build_children(3);
    let sheet = parse_stylesheet("#container { column-count: 3; width: 300px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    for child in &tree.children {
        assert_eq!(child.dimensions.width, 100.0);
    }
}

#[test]
fn an_uneven_split_puts_the_remainder_in_the_last_column() {
    let (d, container) = build_children(5);
    let sheet = parse_stylesheet(
        "#container { column-count: 2; width: 200px; } \
         p { height: 10px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // 5 children / 2 columns = ceil(5/2) = 3 per column: [0,1,2] then [3,4].
    assert_eq!(tree.children[0].dimensions.x, 0.0);
    assert_eq!(tree.children[2].dimensions.x, 0.0);
    assert_eq!(tree.children[3].dimensions.x, 100.0);
    assert_eq!(tree.children[4].dimensions.x, 100.0);
}

#[test]
fn container_height_is_the_tallest_columns_own_height() {
    let (d, container) = build_children(4);
    let sheet = parse_stylesheet(
        "#container { column-count: 2; width: 200px; } \
         p { height: 10px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // Both columns get exactly 2 children of 10px each -> 20px tall.
    assert_eq!(tree.dimensions.height, 20.0);
}

#[test]
fn no_column_count_falls_back_to_plain_block_stacking() {
    let (d, container) = build_children(2);
    let sheet = parse_stylesheet("#container { width: 200px; } p { height: 10px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[1].dimensions.y, 10.0);
    assert_eq!(tree.children[0].dimensions.width, 200.0);
}
