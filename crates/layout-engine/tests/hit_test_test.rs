use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, hit_test, layout_block};

#[test]
fn finds_the_deepest_box_containing_the_point() {
  let mut d = Dom::new();
  let root = d.root();
  let outer = d.create_element("div");
  let inner = d.create_element("div");
  d.append_child(root, outer);
  d.append_child(outer, inner);

  // outer: full width block. inner: a real 50x50 child, offset by
  // margin so it doesn't just coincide with outer's own top-left.
  let sheet = parse_stylesheet(
    r#"
        div { width: 200px; height: 200px; }
        div div { width: 50px; height: 50px; margin: 20px; }
    "#,
  );
  let mut tree = build_box_tree(&d, outer, &sheet).unwrap();
  layout_block(&mut tree, 800.0, 0.0, 0.0);

  let inner_node = tree.children[0].node;
  assert_eq!(inner_node, inner);

  // A point inside the real inner box (offset 20,20 from outer's
  // origin, 50x50) should hit the inner node, not the outer one.
  let hit = hit_test(&tree, 30.0, 30.0).expect("point inside the inner box should hit something");
  assert_eq!(
    hit, inner,
    "a point inside a nested child's real box should resolve to that child, not its parent"
  );

  // A point inside outer but outside inner's real box should hit outer.
  let hit_outer = hit_test(&tree, 150.0, 150.0)
    .expect("point inside outer but outside inner should hit something");
  assert_eq!(hit_outer, outer);
}

#[test]
fn a_point_outside_every_box_returns_none() {
  let mut d = Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.append_child(root, div);

  let sheet = parse_stylesheet("div { width: 100px; height: 100px; }");
  let mut tree = build_box_tree(&d, div, &sheet).unwrap();
  layout_block(&mut tree, 800.0, 0.0, 0.0);

  assert_eq!(
    hit_test(&tree, 9999.0, 9999.0),
    None,
    "a point nowhere near any real box should hit nothing, not the nearest one"
  );
  assert_eq!(
    hit_test(&tree, -5.0, 5.0),
    None,
    "a negative coordinate outside the box should not match"
  );
}

#[test]
fn a_point_exactly_on_the_boxs_own_edge_is_included_but_the_far_edge_is_not() {
  let mut d = Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.append_child(root, div);

  let sheet = parse_stylesheet("div { width: 100px; height: 100px; }");
  let mut tree = build_box_tree(&d, div, &sheet).unwrap();
  layout_block(&mut tree, 800.0, 0.0, 0.0);

  assert_eq!(
    hit_test(&tree, 0.0, 0.0),
    Some(div),
    "the box's own top-left corner should be included"
  );
  assert_eq!(
    hit_test(&tree, 100.0, 50.0),
    None,
    "the box's right edge (x == x + width) is exclusive, matching real rect-containment convention"
  );
}
