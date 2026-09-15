use std::collections::HashMap;
use std::rc::Rc;

use css::parse_stylesheet;
use dom::Dom;
use image_decode::DecodedImage;
use layout_engine::{apply_canvas_snapshots, build_box_tree, Length};

fn fake_snapshot(width: u32, height: u32) -> Rc<DecodedImage> {
    Rc::new(DecodedImage {
        width,
        height,
        rgba: vec![255u8; (width * height * 4) as usize],
    })
}

#[test]
fn a_canvas_with_a_matching_snapshot_gets_its_image_set() {
    let mut d = Dom::new();
    let root = d.root();
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "100");
    d.set_attribute(canvas, "height", "100");
    d.append_child(root, canvas);

    let sheet = parse_stylesheet("canvas { width: 100px; height: 100px; }");
    let mut tree = build_box_tree(&d, canvas, &sheet).unwrap();

    let mut canvases = HashMap::new();
    canvases.insert(canvas, fake_snapshot(100, 100));
    apply_canvas_snapshots(&d, &mut tree, &canvases);

    assert!(tree.image.is_some());
    assert_eq!(tree.image.as_ref().unwrap().width, 100);
}

#[test]
fn unlike_apply_image_sizes_it_never_touches_auto_width_height() {
    let mut d = Dom::new();
    let root = d.root();
    let canvas = d.create_element("canvas");
    d.append_child(root, canvas);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, canvas, &sheet).unwrap();
    assert_eq!(tree.style.width, Length::Auto);
    assert_eq!(tree.style.height, Length::Auto);

    let mut canvases = HashMap::new();
    // A backing pixel buffer resolution unrelated to the box's own
    // (unset) CSS size - proves this function never derives layout size
    // from it, unlike apply_image_sizes's real <img> intrinsic sizing.
    canvases.insert(canvas, fake_snapshot(640, 480));
    apply_canvas_snapshots(&d, &mut tree, &canvases);

    assert_eq!(tree.style.width, Length::Auto);
    assert_eq!(tree.style.height, Length::Auto);
    assert!(tree.image.is_some());
}

#[test]
fn a_canvas_with_no_matching_snapshot_stays_imageless() {
    let mut d = Dom::new();
    let root = d.root();
    let canvas = d.create_element("canvas");
    d.append_child(root, canvas);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, canvas, &sheet).unwrap();
    apply_canvas_snapshots(&d, &mut tree, &HashMap::new());

    assert!(tree.image.is_none());
}

#[test]
fn non_canvas_elements_are_never_affected_even_if_present_in_the_map() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();

    let mut canvases = HashMap::new();
    canvases.insert(div, fake_snapshot(50, 50));
    apply_canvas_snapshots(&d, &mut tree, &canvases);

    assert!(tree.image.is_none());
}

#[test]
fn recurses_into_nested_canvas_descendants() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let canvas = d.create_element("canvas");
    d.append_child(root, container);
    d.append_child(container, canvas);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();

    let mut canvases = HashMap::new();
    canvases.insert(canvas, fake_snapshot(32, 32));
    apply_canvas_snapshots(&d, &mut tree, &canvases);

    let canvas_box = tree
        .children
        .iter()
        .find(|c| c.node == canvas)
        .expect("canvas child box should exist");
    assert!(canvas_box.image.is_some());
}
