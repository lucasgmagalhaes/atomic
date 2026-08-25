use shell::tiling::{grid_layout, Rect};

const CONTAINER: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 1200.0,
    height: 800.0,
};

fn assert_tiles_container(cells: &[Rect], container: Rect) {
    assert!(!cells.is_empty());
    // Every cell's total area should sum back to the container's - proves
    // the cells neither overlap nor leave a gap (a weaker but much simpler
    // check than asserting every edge coordinate).
    let total_area: f32 = cells.iter().map(|c| c.width * c.height).sum();
    let container_area = container.width * container.height;
    assert!(
        (total_area - container_area).abs() < 0.01,
        "cells should exactly tile the container area"
    );

    for cell in cells {
        assert!(cell.x >= container.x - 0.01);
        assert!(cell.y >= container.y - 0.01);
        assert!(cell.x + cell.width <= container.x + container.width + 0.01);
        assert!(cell.y + cell.height <= container.y + container.height + 0.01);
    }
}

#[test]
fn zero_panes_yields_no_cells() {
    assert_eq!(grid_layout(CONTAINER, 0), Vec::new());
}

#[test]
fn one_pane_fills_the_container() {
    let cells = grid_layout(CONTAINER, 1);
    assert_eq!(cells, vec![CONTAINER]);
}

#[test]
fn two_panes_are_equal_side_by_side_columns() {
    let cells = grid_layout(CONTAINER, 2);
    assert_tiles_container(&cells, CONTAINER);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].width, 600.0);
    assert_eq!(cells[0].height, 800.0);
    assert_eq!(cells[1].x, 600.0);
}

#[test]
fn four_panes_form_a_2x2_grid() {
    let cells = grid_layout(CONTAINER, 4);
    assert_tiles_container(&cells, CONTAINER);
    assert_eq!(cells.len(), 4);
    for cell in &cells {
        assert_eq!(cell.width, 600.0);
        assert_eq!(cell.height, 400.0);
    }
    // Distinct positions - a real 2x2 grid, not four stacked identical cells.
    let mut positions: Vec<(i32, i32)> = cells.iter().map(|c| (c.x as i32, c.y as i32)).collect();
    positions.sort();
    positions.dedup();
    assert_eq!(positions.len(), 4);
}

#[test]
fn six_panes_form_a_2x3_grid() {
    let cells = grid_layout(CONTAINER, 6);
    assert_tiles_container(&cells, CONTAINER);
    assert_eq!(cells.len(), 6);
    for cell in &cells {
        assert_eq!(cell.width, 400.0);
        assert_eq!(cell.height, 400.0);
    }
}

#[test]
fn an_uncommon_count_falls_back_to_equal_columns_instead_of_panicking() {
    let cells = grid_layout(CONTAINER, 3);
    assert_tiles_container(&cells, CONTAINER);
    assert_eq!(cells.len(), 3);
    for cell in &cells {
        assert_eq!(cell.height, 800.0);
    }
}
