use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};

#[test]
fn explicit_border_width_grows_the_padding_box_into_a_real_border_box() {
  let mut d = Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.append_child(root, div);

  let sheet = parse_stylesheet("div { width: 100px; height: 50px; border: 10px solid black; }");
  let mut tree = build_box_tree(&d, div, &sheet).unwrap();
  layout_block(&mut tree, 800.0, 0.0, 0.0);

  // 100 content + 10 + 10 border, real box-model growth (not just a
  // paint-time decoration).
  assert_eq!(tree.dimensions.width, 120.0);
  assert_eq!(tree.dimensions.height, 70.0);
}

#[test]
fn auto_width_shrinks_to_leave_room_for_border() {
  let mut d = Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.append_child(root, div);

  let sheet = parse_stylesheet("div { border: 10px solid black; }");
  let mut tree = build_box_tree(&d, div, &sheet).unwrap();
  layout_block(&mut tree, 800.0, 0.0, 0.0);

  // Auto width still fills the containing block exactly (800px total,
  // border included), same "margin + border + padding + width =
  // containing_width" rule this crate already applies to padding.
  assert_eq!(tree.dimensions.width, 800.0);
}

#[test]
fn child_content_origin_shifts_past_the_parents_border() {
  let mut d = Dom::new();
  let root = d.root();
  let parent = d.create_element("div");
  let child = d.create_element("span");
  d.append_child(root, parent);
  d.append_child(parent, child);

  let sheet = parse_stylesheet("div { border: 10px solid black; padding: 5px; }");
  let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
  layout_block(&mut tree, 800.0, 0.0, 0.0);

  // Content starts past both the border and the padding.
  assert_eq!(tree.children[0].dimensions.x, 15.0);
  assert_eq!(tree.children[0].dimensions.y, 15.0);
}

#[test]
fn border_style_none_is_the_default_and_contributes_no_box_model_space() {
  let mut d = Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.append_child(root, div);

  let sheet = parse_stylesheet("div { width: 100px; height: 50px; border-width: 20px; }");
  let mut tree = build_box_tree(&d, div, &sheet).unwrap();
  layout_block(&mut tree, 800.0, 0.0, 0.0);

  // `border-width` alone (no `border-style`) never renders per the real
  // spec - `border_style` defaults to `None`, so the width is ignored
  // for box-model purposes too.
  assert_eq!(tree.dimensions.width, 100.0);
  assert_eq!(tree.dimensions.height, 50.0);
}

#[test]
fn border_shorthand_accepts_its_three_components_in_any_order() {
  let mut d = Dom::new();
  let root = d.root();
  let container = d.create_element("div");
  let a = d.create_element("div");
  let b = d.create_element("div");
  d.append_child(root, container);
  d.append_child(container, a);
  d.append_child(container, b);
  d.set_attribute(a, "id", "a");
  d.set_attribute(b, "id", "b");

  let sheet = parse_stylesheet("#a { border: 4px solid red; } #b { border: solid red 4px; }");
  let mut tree = build_box_tree(&d, container, &sheet).unwrap();
  layout_block(&mut tree, 800.0, 0.0, 0.0);

  assert_eq!(
    tree.children[0].dimensions.width,
    tree.children[1].dimensions.width
  );
  assert_eq!(tree.children[0].border.top, 4.0);
  assert_eq!(tree.children[1].border.top, 4.0);
}

#[test]
fn resolved_border_widths_are_exposed_on_the_layout_box() {
  let mut d = Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.append_child(root, div);

  let sheet = parse_stylesheet(
    "div { border-style: solid; border-width: 1px 2px 3px 4px; border-color: black; }",
  );
  let mut tree = build_box_tree(&d, div, &sheet).unwrap();
  layout_block(&mut tree, 800.0, 0.0, 0.0);

  assert_eq!(tree.border.top, 1.0);
  assert_eq!(tree.border.right, 2.0);
  assert_eq!(tree.border.bottom, 3.0);
  assert_eq!(tree.border.left, 4.0);
}
