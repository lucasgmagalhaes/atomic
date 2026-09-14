//! CSS table layout — real `display: table`/`table-row`/`table-cell`
//! three-level box structure, equal-width columns. See `crate::table`'s
//! own doc for the exact scope cut (no colspan/rowspan, no anonymous box
//! generation, no content-based column sizing).

use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};

fn build_table(rows: &[&[&str]]) -> (Dom, dom::NodeId) {
    let mut d = Dom::new();
    let root = d.root();
    let table = d.create_element("div");
    d.set_attribute(table, "id", "table");
    d.append_child(root, table);
    for row_cells in rows {
        let row = d.create_element("div");
        d.set_attribute(row, "class", "row");
        d.append_child(table, row);
        for _ in *row_cells {
            let cell = d.create_element("div");
            d.set_attribute(cell, "class", "cell");
            d.append_child(row, cell);
        }
    }
    (d, table)
}

#[test]
fn a_single_row_gets_equal_width_columns() {
    let (d, table) = build_table(&[&["a", "b", "c"]]);
    let sheet = parse_stylesheet(
        "#table { display: table; width: 300px; } \
         .row { display: table-row; } \
         .cell { display: table-cell; height: 20px; }",
    );
    let mut tree = build_box_tree(&d, table, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let row = &tree.children[0];
    assert_eq!(row.children.len(), 3);
    assert_eq!(row.children[0].dimensions.x, 0.0);
    assert_eq!(row.children[0].dimensions.width, 100.0);
    assert_eq!(row.children[1].dimensions.x, 100.0);
    assert_eq!(row.children[1].dimensions.width, 100.0);
    assert_eq!(row.children[2].dimensions.x, 200.0);
    assert_eq!(row.children[2].dimensions.width, 100.0);
}

#[test]
fn row_height_is_the_tallest_cell_in_that_row() {
    let mut d = Dom::new();
    let root = d.root();
    let table = d.create_element("div");
    d.set_attribute(table, "id", "table");
    let row = d.create_element("div");
    d.set_attribute(row, "class", "row");
    let short = d.create_element("div");
    let tall = d.create_element("div");
    d.set_attribute(short, "id", "short");
    d.set_attribute(tall, "id", "tall");
    d.append_child(row, short);
    d.append_child(row, tall);
    d.append_child(table, row);
    d.append_child(root, table);

    let sheet = parse_stylesheet(
        "#table { display: table; width: 200px; } \
         .row { display: table-row; } \
         #short { display: table-cell; height: 10px; } \
         #tall { display: table-cell; height: 40px; }",
    );
    let mut tree = build_box_tree(&d, table, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(
        tree.children[0].dimensions.height, 40.0,
        "the row should be as tall as its tallest cell"
    );
}

#[test]
fn column_count_is_the_widest_rows_own_cell_count() {
    let (d, table) = build_table(&[&["a"], &["b", "c", "d"]]);
    let sheet = parse_stylesheet(
        "#table { display: table; width: 300px; } \
         .row { display: table-row; } \
         .cell { display: table-cell; height: 10px; }",
    );
    let mut tree = build_box_tree(&d, table, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // 2nd row has 3 cells - the widest, so every column is 100px, even
    // in the 1st row's single cell.
    assert_eq!(tree.children[0].children[0].dimensions.width, 100.0);
    assert_eq!(tree.children[1].children[0].dimensions.width, 100.0);
    assert_eq!(tree.children[1].children[1].dimensions.width, 100.0);
    assert_eq!(tree.children[1].children[2].dimensions.width, 100.0);
}

#[test]
fn rows_stack_vertically_by_their_own_height() {
    let (d, table) = build_table(&[&["a"], &["b"]]);
    let sheet = parse_stylesheet(
        "#table { display: table; width: 100px; } \
         .row { display: table-row; } \
         .cell { display: table-cell; height: 30px; }",
    );
    let mut tree = build_box_tree(&d, table, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.children[0].dimensions.y, 0.0);
    assert_eq!(tree.children[0].dimensions.height, 30.0);
    assert_eq!(tree.children[1].dimensions.y, 30.0);
    assert_eq!(tree.dimensions.height, 60.0);
}

#[test]
fn an_empty_table_has_zero_height_and_full_content_width() {
    let mut d = Dom::new();
    let root = d.root();
    let table = d.create_element("div");
    d.set_attribute(table, "id", "table");
    d.append_child(root, table);

    let sheet = parse_stylesheet("#table { display: table; width: 400px; }");
    let mut tree = build_box_tree(&d, table, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert_eq!(tree.dimensions.height, 0.0);
    assert_eq!(tree.dimensions.width, 400.0);
}
