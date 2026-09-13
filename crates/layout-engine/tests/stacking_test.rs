//! `layout_engine::stacking` (`ROADMAP.md` item 28's layer-root detection).

use layout_engine::style::{ComputedStyle, Position};
use layout_engine::{
    establishes_stacking_context, find_layer_roots, Dimensions, LayoutBox, ResolvedBorder,
};

fn box_with(node: dom::NodeId, style: ComputedStyle, children: Vec<LayoutBox>) -> LayoutBox {
    LayoutBox {
        node,
        style,
        children,
        dimensions: Dimensions::default(),
        border: ResolvedBorder::default(),
        text: None,
        inline_spans: None,
        glyphs: Vec::new(),
        image: None,
        scroll_offset: (0.0, 0.0),
    }
}

fn leaf(node: dom::NodeId, style: ComputedStyle) -> LayoutBox {
    box_with(node, style, Vec::new())
}

#[test]
fn a_plain_box_is_never_a_layer_root() {
    let d = dom::Dom::new();
    let tree = leaf(d.root(), ComputedStyle::initial());
    assert!(find_layer_roots(&tree).is_empty());
}

#[test]
fn positioned_with_z_index_is_a_layer_root() {
    let mut style = ComputedStyle::initial();
    style.position = Position::Relative;
    style.z_index = Some(1);
    assert!(establishes_stacking_context(&style));
}

#[test]
fn positioned_without_z_index_is_not_a_layer_root() {
    let mut style = ComputedStyle::initial();
    style.position = Position::Relative;
    assert!(!establishes_stacking_context(&style));
}

#[test]
fn translucent_is_a_layer_root() {
    let mut style = ComputedStyle::initial();
    style.opacity = 0.5;
    assert!(establishes_stacking_context(&style));
}

#[test]
fn transformed_is_a_layer_root() {
    let mut style = ComputedStyle::initial();
    style.transform = (10.0, 0.0);
    assert!(establishes_stacking_context(&style));
}

#[test]
fn a_layer_roots_own_descendants_are_excluded_even_if_they_qualify() {
    let mut d = dom::Dom::new();
    let outer_id = d.root();
    let inner_id = d.create_element("div");
    d.append_child(outer_id, inner_id);

    let mut inner_style = ComputedStyle::initial();
    inner_style.opacity = 0.5;
    let inner = leaf(inner_id, inner_style);

    let mut outer_style = ComputedStyle::initial();
    outer_style.opacity = 0.5;
    let tree = box_with(outer_id, outer_style, vec![inner]);

    let roots = find_layer_roots(&tree);
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].node, outer_id);
}

#[test]
fn two_sibling_layer_roots_are_both_found() {
    let mut d = dom::Dom::new();
    let root_id = d.root();
    let a_id = d.create_element("div");
    let b_id = d.create_element("div");
    d.append_child(root_id, a_id);
    d.append_child(root_id, b_id);

    let mut style_a = ComputedStyle::initial();
    style_a.opacity = 0.5;
    let a = leaf(a_id, style_a);

    let mut style_b = ComputedStyle::initial();
    style_b.transform = (5.0, 5.0);
    let b = leaf(b_id, style_b);

    let tree = box_with(root_id, ComputedStyle::initial(), vec![a, b]);

    let roots = find_layer_roots(&tree);
    assert_eq!(roots.len(), 2);
}
