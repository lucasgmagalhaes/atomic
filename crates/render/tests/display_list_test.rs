use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block, Color};
use render::build_display_list;

#[test]
fn transparent_box_produces_no_rect() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert!(build_display_list(&tree).is_empty());
}

#[test]
fn colored_box_produces_one_rect_matching_its_dimensions() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; height: 50px; background-color: red; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].width, 100.0);
    assert_eq!(list[0].height, 50.0);
    assert_eq!(
        list[0].color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn colored_box_with_no_radius_produces_a_zero_radius_rect() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; height: 50px; background-color: red; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].radius, 0.0);
}

#[test]
fn border_radius_flows_onto_the_background_rect() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet(
        "div { width: 100px; height: 50px; background-color: red; border-radius: 12px; }",
    );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].radius, 12.0);
}

#[test]
fn parent_paints_before_children_so_children_can_draw_on_top() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let sheet = parse_stylesheet(
        "div { background-color: blue; } span { background-color: red; height: 10px; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(
        list[0].color,
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }
    );
    assert_eq!(
        list[1].color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn only_colored_descendants_are_collected_among_transparent_ones() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let transparent_child = d.create_element("span");
    let colored_grandchild = d.create_element("em");
    d.append_child(root, parent);
    d.append_child(parent, transparent_child);
    d.append_child(transparent_child, colored_grandchild);

    let sheet = parse_stylesheet("em { background-color: #00ff00; height: 5px; }");
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].color,
        Color {
            r: 0,
            g: 255,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn negative_z_index_paints_behind_a_later_non_positioned_sibling() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let normal = d.create_element("span");
    let behind = d.create_element("em");
    d.append_child(root, parent);
    d.append_child(parent, normal);
    d.append_child(parent, behind);

    let sheet = parse_stylesheet(
        "span { width: 10px; height: 10px; background-color: red; } \
         em { width: 10px; height: 10px; background-color: blue; position: relative; z-index: -1; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // `em` comes second in tree order but has a negative z-index, so it
    // must paint first (furthest back) despite that.
    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].color.b, 255);
    assert_eq!(list[1].color.r, 255);
}

#[test]
fn positive_z_index_paints_in_front_of_an_earlier_non_positioned_sibling() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let front = d.create_element("em");
    let normal = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, front);
    d.append_child(parent, normal);

    let sheet = parse_stylesheet(
        "em { width: 10px; height: 10px; background-color: blue; position: relative; z-index: 1; } \
         span { width: 10px; height: 10px; background-color: red; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // `em` comes first in tree order but has a positive z-index, so it
    // must paint last (on top) despite that.
    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].color.r, 255);
    assert_eq!(list[1].color.b, 255);
}

#[test]
fn equal_z_index_siblings_keep_their_tree_order() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let first = d.create_element("em");
    let second = d.create_element("i");
    d.append_child(root, parent);
    d.append_child(parent, first);
    d.append_child(parent, second);

    let sheet = parse_stylesheet(
        "em { width: 10px; height: 10px; background-color: red; position: relative; z-index: 1; } \
         i { width: 10px; height: 10px; background-color: blue; position: relative; z-index: 1; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].color.r, 255);
    assert_eq!(list[1].color.b, 255);
}

#[test]
fn z_index_without_position_never_establishes_a_stacking_context() {
    // `z-index` alone (no `position`) is a documented no-op per the real
    // spec's own trigger (`position != static` *and* an explicit
    // z-index) - a `static` box with z-index must paint in plain tree
    // order, same as if it had none at all.
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let first = d.create_element("em");
    let second = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, first);
    d.append_child(parent, second);

    let sheet = parse_stylesheet(
        "em { width: 10px; height: 10px; background-color: red; z-index: 5; } \
         span { width: 10px; height: 10px; background-color: blue; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].color.r, 255);
    assert_eq!(list[1].color.b, 255);
}

#[test]
fn bordered_box_produces_four_real_border_strips_around_its_edge() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; height: 50px; border: 5px solid blue; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // No background here - just the 4 border strips. `dimensions` is the
    // real border box (100 + 5 + 5 wide, 50 + 5 + 5 tall - see
    // `layout_engine::layout::layout_block`'s own doc on real box-model
    // growth from `border-width`).
    assert_eq!(tree.dimensions.width, 110.0);
    assert_eq!(tree.dimensions.height, 60.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 4);
    for rect in &list {
        assert_eq!(
            rect.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255
            }
        );
    }
    // top
    assert!(list
        .iter()
        .any(|r| r.x == 0.0 && r.y == 0.0 && r.width == 110.0 && r.height == 5.0));
    // bottom
    assert!(list
        .iter()
        .any(|r| r.x == 0.0 && r.y == 55.0 && r.width == 110.0 && r.height == 5.0));
    // left
    assert!(list
        .iter()
        .any(|r| r.x == 0.0 && r.y == 0.0 && r.width == 5.0 && r.height == 60.0));
    // right
    assert!(list
        .iter()
        .any(|r| r.x == 105.0 && r.y == 0.0 && r.width == 5.0 && r.height == 60.0));
}

#[test]
fn border_style_none_produces_no_border_rects_even_with_a_width_set() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet(
    "div { width: 100px; height: 50px; border-width: 5px; border-style: none; border-color: red; }",
  );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert!(build_display_list(&tree).is_empty());
    // Real spec rule: no style means the width contributes nothing to the
    // box model either, not just to painting.
    assert_eq!(tree.dimensions.width, 100.0);
}

#[test]
fn zero_width_border_side_contributes_no_rect() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; height: 50px; border-style: solid; border-color: green; border-width: 0px 3px 0px 3px; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    // Only left/right (3px each) - top/bottom are 0px and contribute
    // nothing, same as a transparent background contributing no rect.
    assert_eq!(list.len(), 2);
}

#[test]
fn background_and_border_paint_in_the_right_order_for_a_bordered_colored_box() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet(
        "div { width: 100px; height: 50px; background-color: #ffff00; border: 2px solid black; }",
    );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    // Background first (so the border strips paint on top of it, real
    // paint order), then the 4 border strips.
    assert_eq!(list.len(), 5);
    assert_eq!(
        list[0].color,
        Color {
            r: 255,
            g: 255,
            b: 0,
            a: 255
        }
    );
    for rect in &list[1..] {
        assert_eq!(
            rect.color,
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255
            }
        );
    }
}

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

#[test]
fn box_shadow_paints_one_offset_rect_behind_the_box() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet(
        "div { width: 50px; height: 50px; background-color: #ffff00; box-shadow: 5px 10px blue; }",
    );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    // Shadow first (paints behind, per real box-shadow stacking), then
    // the box's own background on top of it.
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].x, 5.0);
    assert_eq!(list[0].y, 10.0);
    assert_eq!(list[0].width, 50.0);
    assert_eq!(list[0].height, 50.0);
    assert_eq!(
        list[0].color,
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }
    );
    assert_eq!(
        list[1].color,
        Color {
            r: 255,
            g: 255,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn box_shadow_spread_grows_the_shadow_rect_on_every_side() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet =
        parse_stylesheet("div { width: 50px; height: 50px; box-shadow: 0px 0px 0px 5px red; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].x, -5.0);
    assert_eq!(list[0].y, -5.0);
    assert_eq!(list[0].width, 60.0);
    assert_eq!(list[0].height, 60.0);
}

#[test]
fn box_shadow_none_produces_no_shadow_rect() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 50px; height: 50px; box-shadow: none; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert!(build_display_list(&tree).is_empty());
}

#[test]
fn box_shadow_is_clipped_by_an_overflow_hidden_ancestor() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let sheet = parse_stylesheet(
        "div { width: 20px; height: 20px; overflow: hidden; } \
         span { width: 20px; height: 20px; box-shadow: 100px 100px red; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    // The shadow is offset 100px past the 20x20 clip region - entirely
    // clipped away, same as any other paint primitive under an
    // overflow: hidden ancestor.
    assert!(build_display_list(&tree).is_empty());
}

#[test]
fn opacity_scales_a_boxs_own_background_alpha() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet =
        parse_stylesheet("div { width: 10px; height: 10px; background-color: red; opacity: 0.5; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 128
        }
    );
}

#[test]
fn opacity_compounds_multiplicatively_through_nested_ancestors() {
    let mut d = Dom::new();
    let root = d.root();
    let outer = d.create_element("div");
    let inner = d.create_element("span");
    d.append_child(root, outer);
    d.append_child(outer, inner);

    // Real nested-opacity semantics: a 50% child inside a 50% parent
    // renders at an effective 25% (0.5 * 0.5), even though the child's
    // own *computed* `opacity` is independently 0.5 (opacity isn't a
    // CSS-inherited property - see `crates/layout-engine/tests/style_test.rs`'s
    // own coverage of that at the ComputedStyle level; this is testing
    // the separate, real render-time multiplicative compounding this
    // crate uses instead of true group compositing).
    let sheet = parse_stylesheet(
        "div { opacity: 0.5; } \
         span { width: 10px; height: 10px; background-color: red; opacity: 0.5; }",
    );
    let mut tree = build_box_tree(&d, outer, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    // 255 * 0.5 * 0.5 = 63.75, rounds to 64.
    assert_eq!(
        list[0].color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 64
        }
    );
}

#[test]
fn opacity_scales_the_box_shadow_and_border_alpha_too() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet(
        "div { width: 10px; height: 10px; border: 2px solid blue; box-shadow: 5px 5px red; opacity: 0.5; }",
    );
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    // Shadow rect, then 4 border strips (no background set).
    assert_eq!(list.len(), 5);
    assert_eq!(
        list[0].color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 128
        }
    );
    for rect in &list[1..] {
        assert_eq!(
            rect.color,
            Color {
                r: 0,
                g: 0,
                b: 255,
                a: 128
            }
        );
    }
}

#[test]
fn opacity_one_leaves_colors_unchanged() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 10px; height: 10px; background-color: red; }");
    let mut tree = build_box_tree(&d, div, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(
        list[0].color,
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}
