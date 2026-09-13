use dom::{DirtyFlags, Dom};

#[test]
fn mutation_count_changes_on_structural_and_content_mutations_only() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    let before_append = dom.mutation_count();
    dom.append_child(root, el);
    assert_ne!(dom.mutation_count(), before_append);

    let before_attr = dom.mutation_count();
    dom.set_attribute(el, "class", "x");
    assert_ne!(dom.mutation_count(), before_attr);

    let before_text = dom.mutation_count();
    dom.set_text_content(el, "hello");
    assert_ne!(dom.mutation_count(), before_text);

    let before_remove = dom.mutation_count();
    dom.remove(el);
    assert_ne!(dom.mutation_count(), before_remove);

    // focus/blur don't affect anything layout-engine computes - must not
    // bump the counter, or a caller using it to skip relayout would
    // relayout on every keystroke's focus churn for no reason.
    let other = dom.create_element("input");
    dom.append_child(root, other);
    dom.focus(other);
    let before_blur = dom.mutation_count();
    dom.blur(other);
    assert_eq!(dom.mutation_count(), before_blur);
}

#[test]
fn dirty_flags_classify_structural_mutations() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");

    // Structural mutation sets all flags.
    dom.append_child(root, el);
    let flags = dom.drain_dirty();
    assert!(flags.contains(DirtyFlags::DOM));
    assert!(flags.contains(DirtyFlags::COLLECTION));
    assert!(flags.contains(DirtyFlags::SELECTORS));
    assert!(flags.contains(DirtyFlags::STYLE));
    assert!(flags.contains(DirtyFlags::LAYOUT));
    assert!(flags.contains(DirtyFlags::PAINT));
    assert!(flags.contains(DirtyFlags::A11Y));
}

#[test]
fn dirty_flags_classify_attribute_mutations() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    dom.append_child(root, el);
    dom.drain_dirty(); // clear structural flags

    dom.set_attribute(el, "class", "x");
    let flags = dom.drain_dirty();
    assert!(!flags.contains(DirtyFlags::DOM));
    assert!(!flags.contains(DirtyFlags::COLLECTION));
    assert!(flags.contains(DirtyFlags::SELECTORS));
    assert!(flags.contains(DirtyFlags::STYLE));
    assert!(flags.contains(DirtyFlags::LAYOUT));
    assert!(flags.contains(DirtyFlags::PAINT));
    assert!(!flags.contains(DirtyFlags::A11Y));
}

#[test]
fn dirty_flags_classify_text_mutations() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    dom.append_child(root, el);
    dom.drain_dirty();

    dom.set_text_content(el, "hello");
    let flags = dom.drain_dirty();
    // set_text_content calls remove + create_text + append_child internally,
    // so DOM + LAYOUT + PAINT are set (plus STRUCTURAL from the inner calls).
    assert!(flags.contains(DirtyFlags::DOM));
    assert!(flags.contains(DirtyFlags::LAYOUT));
    assert!(flags.contains(DirtyFlags::PAINT));
}

#[test]
fn drain_dirty_resets_flags() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    dom.append_child(root, el);

    let first = dom.drain_dirty();
    assert!(!first.is_empty());

    let second = dom.drain_dirty();
    assert!(second.is_empty());
}

#[test]
fn focus_blur_do_not_set_dirty_flags() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("input");
    dom.append_child(root, el);
    dom.drain_dirty();

    dom.focus(el);
    assert!(dom.drain_dirty().is_empty());

    dom.blur(el);
    assert!(dom.drain_dirty().is_empty());
}

#[test]
fn layout_version_bumps_only_on_layout_relevant_mutations() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    dom.append_child(root, el);

    let v0 = dom.layout_version();
    assert_eq!(v0, 1); // append_child sets LAYOUT

    // Attribute mutation: LAYOUT flag is set (conservative — the DOM
    // layer can't know which attributes affect layout), so
    // layout_version bumps.
    dom.set_attribute(el, "data-x", "1");
    assert_eq!(dom.layout_version(), v0 + 1);

    // Structural mutation: LAYOUT flag set → layout_version bumps.
    let el2 = dom.create_element("span");
    dom.append_child(root, el2);
    assert_eq!(dom.layout_version(), v0 + 2);

    // focus/blur: no dirty flags → layout_version unchanged.
    dom.focus(el);
    assert_eq!(dom.layout_version(), v0 + 2);
    dom.blur(el);
    assert_eq!(dom.layout_version(), v0 + 2);
}
