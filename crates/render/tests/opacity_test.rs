use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block, Color};
use render::build_display_list;

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
