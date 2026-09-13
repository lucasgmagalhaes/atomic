//! Real per-element scroll shift in `render::display_list` (`ROADMAP.md`
//! item 22) — a scrolled `overflow: hidden`/`auto`/`scroll` container's
//! own children shift by its `LayoutBox::scroll_offset`, while the
//! container's own background/border stay fixed, still clipped to its
//! own border box (`overflow_test.rs`'s existing clip behavior, unchanged).

use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};
use render::{build_display_list, build_glyph_list, build_image_list};

#[test]
fn a_scrolled_containers_child_rect_shifts_by_the_scroll_offset() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let first = d.create_element("span");
    let second = d.create_element("p");
    d.append_child(root, parent);
    d.append_child(parent, first);
    d.append_child(parent, second);
    d.set_element_scroll_offset(parent, 0.0, 20.0);

    // Two stacked 30px-tall children (real content height 60px) inside a
    // 50px-tall clipped container. Scrolled down 20px: `first` (real y 0)
    // shifts to -20 (clipped to the container's own y=0, so its clamped
    // appearance alone wouldn't prove the shift happened) - `second`
    // (real y 30) shifts to 30 - 20 = 10, fully inside the clip with no
    // clamping, so its exact painted y is the real, unambiguous proof.
    let sheet = parse_stylesheet(
        "div { width: 50px; height: 50px; overflow: hidden; } \
         span { width: 50px; height: 30px; background-color: red; } \
         p { width: 50px; height: 30px; background-color: blue; margin: 0; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 2);
    assert_eq!(
        list[1].y, 10.0,
        "second child must paint at its real 30px position minus the 20px scroll offset"
    );
    assert_eq!(
        list[1].height, 30.0,
        "fully inside the clip region, not clamped at all"
    );
}

#[test]
fn a_scrolled_containers_own_background_does_not_move() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.append_child(root, parent);
    d.set_element_scroll_offset(parent, 0.0, 40.0);

    let sheet = parse_stylesheet(
        "div { width: 50px; height: 50px; overflow: hidden; background-color: blue; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].x, 0.0);
    assert_eq!(
        list[0].y, 0.0,
        "the container's own background must not scroll with its content"
    );
}

#[test]
fn content_scrolled_entirely_out_of_view_is_dropped() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);
    d.set_element_scroll_offset(parent, 0.0, 500.0);

    let sheet = parse_stylesheet(
        "div { width: 50px; height: 50px; overflow: hidden; } \
         span { width: 50px; height: 20px; background-color: red; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    assert!(build_display_list(&tree).is_empty());
}

#[test]
fn an_unscrolled_container_behaves_exactly_as_before() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let sheet = parse_stylesheet(
        "div { width: 50px; height: 50px; overflow: hidden; } \
         span { width: 50px; height: 20px; background-color: red; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let list = build_display_list(&tree);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].y, 0.0);
    assert_eq!(list[0].height, 20.0);
}

#[test]
fn scroll_shift_also_applies_to_images_and_glyphs() {
    fn build_tree(
        d: &Dom,
        parent: dom::NodeId,
        sheet: &css::Stylesheet,
    ) -> layout_engine::LayoutBox {
        let mut tree = build_box_tree(d, parent, sheet).unwrap();
        layout_block(&mut tree, 800.0, 0.0, 0.0);
        tree
    }

    let sheet = parse_stylesheet(
        "div { width: 100px; height: 100px; overflow: hidden; } \
         img { width: 10px; height: 10px; }",
    );

    let mut unscrolled = Dom::new();
    let root = unscrolled.root();
    let parent = unscrolled.create_element("div");
    let img = unscrolled.create_element("img");
    let text_node = unscrolled.create_text("hello");
    unscrolled.append_child(root, parent);
    unscrolled.append_child(parent, img);
    unscrolled.append_child(parent, text_node);
    let baseline_glyphs = build_glyph_list(&build_tree(&unscrolled, parent, &sheet));

    let mut scrolled = Dom::new();
    let root = scrolled.root();
    let parent = scrolled.create_element("div");
    let img = scrolled.create_element("img");
    let text_node = scrolled.create_text("hello");
    scrolled.append_child(root, parent);
    scrolled.append_child(parent, img);
    scrolled.append_child(parent, text_node);
    scrolled.set_element_scroll_offset(parent, 0.0, 10.0);
    let scrolled_tree = build_tree(&scrolled, parent, &sheet);
    let scrolled_glyphs = build_glyph_list(&scrolled_tree);

    // No decoded image is attached (apply_image_sizes never ran), so the
    // image list stays empty regardless - this test only proves the
    // glyph path shifts too, which doesn't need a decoded image.
    let _ = build_image_list(&scrolled_tree);

    assert!(
        !baseline_glyphs.is_empty(),
        "text inside a container must shape real glyphs"
    );
    assert_eq!(baseline_glyphs.len(), scrolled_glyphs.len());
    for (base, scrolled) in baseline_glyphs.iter().zip(scrolled_glyphs.iter()) {
        assert_eq!(
            scrolled.glyph.y,
            base.glyph.y - 10,
            "each glyph must paint exactly 10px higher once its container scrolls down 10px"
        );
    }
}
