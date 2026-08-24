use std::collections::HashMap;
use std::rc::Rc;

use css::parse_stylesheet;
use dom::Dom;
use image_decode::DecodedImage;
use layout_engine::{apply_image_sizes, build_box_tree, Length};

fn fake_decoded_image(width: u32, height: u32) -> Rc<DecodedImage> {
    Rc::new(DecodedImage {
        width,
        height,
        rgba: vec![255u8; (width * height * 4) as usize],
    })
}

#[test]
fn img_with_no_explicit_css_size_gets_the_real_intrinsic_dimensions() {
    let mut d = Dom::new();
    let root = d.root();
    let img = d.create_element("img");
    d.set_attribute(img, "src", "photo.png");
    d.append_child(root, img);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, img, &sheet).unwrap();
    assert_eq!(tree.style.width, Length::Auto);
    assert_eq!(tree.style.height, Length::Auto);

    let mut images = HashMap::new();
    images.insert(img, fake_decoded_image(320, 200));
    apply_image_sizes(&d, &mut tree, &images);

    assert_eq!(tree.style.width, Length::Px(320.0));
    assert_eq!(tree.style.height, Length::Px(200.0));
    assert!(tree.image.is_some());
    assert_eq!(tree.image.as_ref().unwrap().width, 320);
}

#[test]
fn img_with_explicit_css_width_keeps_it_over_the_intrinsic_size() {
    let mut d = Dom::new();
    let root = d.root();
    let img = d.create_element("img");
    d.set_attribute(img, "src", "photo.png");
    d.append_child(root, img);

    let sheet = parse_stylesheet("img { width: 50px; }");
    let mut tree = build_box_tree(&d, img, &sheet).unwrap();
    assert_eq!(tree.style.width, Length::Px(50.0));

    let mut images = HashMap::new();
    images.insert(img, fake_decoded_image(320, 200));
    apply_image_sizes(&d, &mut tree, &images);

    // CSS width wins - real intrinsic size only fills in what CSS left
    // as `Auto`, matching real browser behavior.
    assert_eq!(tree.style.width, Length::Px(50.0));
    // Height still had nothing explicit, so it does pick up the real
    // intrinsic value.
    assert_eq!(tree.style.height, Length::Px(200.0));
    assert!(tree.image.is_some());
}

#[test]
fn img_with_no_matching_entry_in_the_images_map_stays_unsized_and_imageless() {
    let mut d = Dom::new();
    let root = d.root();
    let img = d.create_element("img");
    d.set_attribute(img, "src", "broken.png");
    d.append_child(root, img);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, img, &sheet).unwrap();

    apply_image_sizes(&d, &mut tree, &HashMap::new());

    assert_eq!(tree.style.width, Length::Auto);
    assert_eq!(tree.style.height, Length::Auto);
    assert!(tree.image.is_none());
}

#[test]
fn non_img_elements_are_never_affected_even_if_present_in_the_images_map() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();

    let mut images = HashMap::new();
    images.insert(div, fake_decoded_image(100, 100));
    apply_image_sizes(&d, &mut tree, &images);

    assert_eq!(tree.style.width, Length::Auto);
    assert!(tree.image.is_none());
}

#[test]
fn recurses_into_nested_img_descendants() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    let img = d.create_element("img");
    d.set_attribute(img, "src", "nested.png");
    d.append_child(root, container);
    d.append_child(container, img);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();

    let mut images = HashMap::new();
    images.insert(img, fake_decoded_image(64, 48));
    apply_image_sizes(&d, &mut tree, &images);

    let img_box = tree.children.iter().find(|c| c.node == img).expect("img child box should exist");
    assert_eq!(img_box.style.width, Length::Px(64.0));
    assert_eq!(img_box.style.height, Length::Px(48.0));
}
