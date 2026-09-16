use dom::{DirtyFlags, Dom};

#[test]
fn a_never_scrolled_element_reads_zero_offset() {
    let dom = Dom::new();
    assert_eq!(dom.element_scroll_offset(dom.root()), (0.0, 0.0));
}

#[test]
fn set_and_get_round_trip() {
    let mut dom = Dom::new();
    let root = dom.root();
    dom.set_element_scroll_offset(root, 10.0, 20.0);
    assert_eq!(dom.element_scroll_offset(root), (10.0, 20.0));
}

#[test]
fn setting_scroll_offset_on_a_nonexistent_node_is_a_no_op() {
    let mut dom = Dom::new();
    let root = dom.root();
    let child = dom.create_element("div");
    dom.append_child(root, child);
    dom.remove(child); // frees the slot - `child` is now a stale id
    dom.set_element_scroll_offset(child, 5.0, 5.0);
    assert_eq!(dom.element_scroll_offset(child), (0.0, 0.0));
    // unrelated node unaffected
    assert_eq!(dom.element_scroll_offset(root), (0.0, 0.0));
}

#[test]
fn setting_scroll_offset_never_bumps_layout_version_but_does_bump_style_version() {
    let mut dom = Dom::new();
    let root = dom.root();
    let layout_before = dom.layout_version();
    let style_before = dom.style_version();
    dom.drain_dirty();

    dom.set_element_scroll_offset(root, 3.0, 4.0);

    assert_eq!(
        dom.layout_version(),
        layout_before,
        "scrolling must not bump layout_version - boxes keep their real positions"
    );
    // style_version bumps even though nothing about selector matching
    // changed - it's the real signal a host's whole-frame PaintCache is
    // keyed on (see `set_element_scroll_offset`'s own doc), the same
    // mechanism `:hover`/`:focus` already rely on to avoid a stale cache.
    assert_ne!(
        dom.style_version(),
        style_before,
        "scrolling must bump style_version so a host's PaintCache invalidates"
    );
    let dirty = dom.drain_dirty();
    assert!(
        dirty.contains(DirtyFlags::PAINT),
        "scrolling must set the PAINT dirty flag"
    );
    assert!(
        !dirty.contains(DirtyFlags::LAYOUT),
        "scrolling must not set the LAYOUT dirty flag"
    );
}
