//! CSS Grid layout (`ROADMAP.md` item 34) — real fixed-track grid with
//! `px`/`fr` tracks and row-major auto-placement. See `crate::grid`'s own
//! doc for the scope cut (no `grid-column`/`grid-row` placement, no
//! `grid-template-areas`, no `gap`).

use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};

fn build(children_tags: &[&str]) -> (Dom, dom::NodeId) {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    d.append_child(root, container);
    for tag in children_tags {
        let child = d.create_element(tag);
        d.append_child(container, child);
    }
    (d, container)
}

#[test]
fn two_fixed_columns_place_items_side_by_side() {
    let (d, container) = build(&["span", "p"]);
    let sheet = parse_stylesheet(
        "div { display: grid; grid-template-columns: 100px 200px; } \
         span, p { height: 50px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.x, 0.0);
    assert_eq!(tree.children[0].dimensions.width, 100.0);
    assert_eq!(tree.children[1].dimensions.x, 100.0);
    assert_eq!(tree.children[1].dimensions.width, 200.0);
    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[1].dimensions.y, 0.0);
}

#[test]
fn fr_tracks_distribute_remaining_width_by_share() {
    let (d, container) = build(&["span", "p"]);
    let sheet = parse_stylesheet(
        "div { display: grid; grid-template-columns: 1fr 2fr; width: 900px; } \
         span, p { height: 10px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.width, 300.0);
    assert_eq!(tree.children[1].dimensions.width, 600.0);
    assert_eq!(tree.children[1].dimensions.x, 300.0);
}

#[test]
fn a_mix_of_fixed_and_fr_tracks_distributes_only_the_leftover() {
    let (d, container) = build(&["span", "p"]);
    let sheet = parse_stylesheet(
        "div { display: grid; grid-template-columns: 200px 1fr; width: 800px; } \
         span, p { height: 10px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.width, 200.0);
    assert_eq!(tree.children[1].dimensions.width, 600.0);
}

#[test]
fn items_wrap_to_a_new_row_after_every_column_is_filled() {
    let (d, container) = build(&["span", "p", "b"]);
    let sheet = parse_stylesheet(
        "div { display: grid; grid-template-columns: 100px 100px; } \
         span, p, b { height: 50px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // Row 0: span, p. Row 1: b (wraps back to column 0).
    assert_eq!(tree.children[0].dimensions.x, 0.0);
    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[1].dimensions.x, 100.0);
    assert_eq!(tree.children[1].dimensions.y, 0.0);
    assert_eq!(tree.children[2].dimensions.x, 0.0);
    assert_eq!(tree.children[2].dimensions.y, 50.0);
}

#[test]
fn a_rows_height_hugs_its_tallest_item() {
    let (d, container) = build(&["span", "p", "b", "i"]);
    let sheet = parse_stylesheet(
        "div { display: grid; grid-template-columns: 100px 100px; } \
         span { height: 30px; } p { height: 90px; } \
         b { height: 10px; } i { height: 10px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // Row 0's tallest item (p, 90px) sets the row 1 items' y offset.
    assert_eq!(tree.children[2].dimensions.y, 90.0);
    assert_eq!(tree.children[3].dimensions.y, 90.0);
}

#[test]
fn explicit_row_tracks_size_rows_instead_of_hugging_content() {
    let (d, container) = build(&["span", "p"]);
    let sheet = parse_stylesheet(
        "div { display: grid; grid-template-columns: 100px; grid-template-rows: 60px 60px; } \
         span, p { height: 10px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[1].dimensions.y, 60.0);
}

#[test]
fn no_explicit_template_columns_falls_back_to_a_single_full_width_column() {
    let (d, container) = build(&["span", "p"]);
    let sheet = parse_stylesheet("div { display: grid; width: 400px; } span, p { height: 20px; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.width, 400.0);
    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[1].dimensions.y, 20.0);
}

#[test]
fn margin_on_a_grid_item_shrinks_it_within_its_cell() {
    let (d, container) = build(&["span"]);
    let sheet = parse_stylesheet(
        "div { display: grid; grid-template-columns: 100px; } \
         span { height: 10px; margin: 10px; }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.x, 10.0);
    assert_eq!(tree.children[0].dimensions.width, 80.0);
}

#[test]
fn nested_grid_lays_out_grandchildren() {
    let mut d = Dom::new();
    let root = d.root();
    let outer = d.create_element("div");
    let inner = d.create_element("section");
    let leaf = d.create_element("span");
    d.append_child(root, outer);
    d.append_child(outer, inner);
    d.append_child(inner, leaf);

    let sheet = parse_stylesheet(
        "div { display: grid; grid-template-columns: 300px; } \
         section { display: grid; grid-template-columns: 60px; } \
         span { height: 20px; }",
    );
    let mut tree = build_box_tree(&d, outer, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let inner_box = &tree.children[0];
    assert_eq!(inner_box.dimensions.width, 300.0);
    let leaf_box = &inner_box.children[0];
    assert_eq!(leaf_box.dimensions.width, 60.0);
}
