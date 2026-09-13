use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};
use render::build_display_list;

#[test]
fn overflow_hidden_clips_a_child_that_extends_past_its_bounds() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let sheet = parse_stylesheet(
        "div { width: 50px; height: 50px; overflow: hidden; } \
         span { width: 100px; height: 100px; background-color: red; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].x, 0.0);
    assert_eq!(list[0].y, 0.0);
    // Clipped down to the parent's own bounds, not the child's real
    // (much larger) size.
    assert_eq!(list[0].width, 50.0);
    assert_eq!(list[0].height, 50.0);
}

#[test]
fn overflow_visible_does_not_clip_children() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    // Default `overflow: visible` - explicit here to make the contrast
    // with the `hidden` case above obvious, though `visible` is also the
    // initial value.
    let sheet = parse_stylesheet(
        "div { width: 50px; height: 50px; overflow: visible; } \
         span { width: 100px; height: 100px; background-color: red; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].width, 100.0);
    assert_eq!(list[0].height, 100.0);
}

#[test]
fn overflow_hidden_does_not_clip_the_box_itself_only_its_descendants() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    // A box's own background/border paint at their real (unclipped) size
    // even when that same box sets `overflow: hidden` - real CSS clips a
    // box's *content*, not the box's own border box.
    let sheet = parse_stylesheet(
        "div { width: 50px; height: 50px; overflow: hidden; background-color: blue; }",
    );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].width, 50.0);
    assert_eq!(list[0].height, 50.0);
}

#[test]
fn a_descendant_entirely_outside_the_clip_region_is_dropped() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    // `position: relative; left: 100px` shifts the 20x20 child fully past
    // the parent's 50px-wide clipped bounds - no partial overlap left to
    // clip down to, so it contributes no rect at all (real behavior: a
    // fully-clipped-away element paints nothing, same as a transparent
    // one).
    let sheet = parse_stylesheet(
        "div { width: 50px; height: 50px; overflow: hidden; } \
         span { width: 20px; height: 20px; background-color: red; position: relative; left: 100px; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert!(build_display_list(&tree).is_empty());
}

#[test]
fn overflow_hidden_clip_propagates_through_a_nested_ancestor() {
    let mut d = Dom::new();
    let root = d.root();
    let outer = d.create_element("div");
    let inner = d.create_element("div");
    let grandchild = d.create_element("span");
    d.append_child(root, outer);
    d.append_child(outer, inner);
    d.append_child(inner, grandchild);
    d.set_attribute(outer, "id", "outer");

    // `outer` clips to 30x30; `inner` is unclipped itself (`overflow:
    // visible`, the default) but sits inside `outer`'s clip, so
    // `grandchild` - real size 100x100 - still gets clipped down to
    // `outer`'s bounds even though its immediate parent doesn't clip at
    // all. Tests that the clip accumulates down the ancestor chain, not
    // just from the immediate parent.
    let sheet = parse_stylesheet(
        "#outer { width: 30px; height: 30px; overflow: hidden; } \
         span { width: 100px; height: 100px; background-color: red; }",
    );
    let mut tree = build_box_tree(&d, outer, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].width, 30.0);
    assert_eq!(list[0].height, 30.0);
}
