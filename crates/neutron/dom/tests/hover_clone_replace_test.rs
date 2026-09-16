use dom::Dom;

#[test]
fn hover_set_and_clear_round_trip() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    dom.append_child(root, el);

    assert_eq!(dom.hovered_element(), None);
    dom.set_hovered(el);
    assert_eq!(dom.hovered_element(), Some(el));
    dom.clear_hover();
    assert_eq!(dom.hovered_element(), None);
}

#[test]
fn hover_never_bumps_layout_version() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    dom.append_child(root, el);

    let v0 = dom.layout_version();
    dom.set_hovered(el);
    assert_eq!(dom.layout_version(), v0);
    dom.clear_hover();
    assert_eq!(dom.layout_version(), v0);
}

#[test]
fn hover_and_focus_bump_style_version_layout_does_not() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    dom.append_child(root, el);

    let s0 = dom.style_version();
    dom.set_hovered(el);
    assert_eq!(dom.style_version(), s0 + 1);
    // Setting the same node again is a no-op (already hovered).
    dom.set_hovered(el);
    assert_eq!(dom.style_version(), s0 + 1);
    dom.clear_hover();
    assert_eq!(dom.style_version(), s0 + 2);

    dom.focus(el);
    assert_eq!(dom.style_version(), s0 + 3);
    dom.blur(el);
    assert_eq!(dom.style_version(), s0 + 4);

    // A structural, layout-relevant mutation never bumps style_version.
    let s_before_structural = dom.style_version();
    let el2 = dom.create_element("span");
    dom.append_child(root, el2);
    assert_eq!(dom.style_version(), s_before_structural);
}

#[test]
fn contains_is_true_for_self_and_descendants_false_otherwise() {
    let mut dom = Dom::new();
    let root = dom.root();
    let parent = dom.create_element("div");
    let child = dom.create_element("span");
    let unrelated = dom.create_element("p");
    dom.append_child(root, parent);
    dom.append_child(parent, child);
    dom.append_child(root, unrelated);

    assert!(dom.contains(parent, parent));
    assert!(dom.contains(parent, child));
    assert!(dom.contains(root, child));
    assert!(!dom.contains(parent, unrelated));
    assert!(!dom.contains(child, parent));
}

#[test]
fn is_connected_is_true_only_while_attached_to_the_document_root() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");

    assert!(!dom.is_connected(el));

    dom.append_child(root, el);
    assert!(dom.is_connected(el));

    dom.remove_from_parent(el);
    assert!(!dom.is_connected(el));
}

#[test]
fn clone_node_shallow_omits_children_deep_includes_them() {
    let mut dom = Dom::new();
    let root = dom.root();
    let parent = dom.create_element("div");
    dom.set_attribute(parent, "class", "widget");
    let child = dom.create_text("hello");
    dom.append_child(root, parent);
    dom.append_child(parent, child);

    let shallow = dom.clone_node(parent, false);
    assert_eq!(dom.attribute(shallow, "class"), Some("widget"));
    assert!(dom.get(shallow).unwrap().children.is_empty());
    assert_eq!(dom.get(shallow).unwrap().parent, None);

    let deep = dom.clone_node(parent, true);
    assert_eq!(dom.text_content(deep), "hello");
    assert_eq!(dom.attribute(deep, "class"), Some("widget"));

    // Mutating the clone must not affect the original.
    dom.set_attribute(deep, "class", "changed");
    assert_eq!(dom.attribute(parent, "class"), Some("widget"));
}

#[test]
fn replace_child_swaps_at_the_same_position_and_keeps_the_old_child_alive() {
    let mut dom = Dom::new();
    let root = dom.root();
    let first = dom.create_element("a");
    let old = dom.create_element("b");
    let last = dom.create_element("c");
    dom.append_child(root, first);
    dom.append_child(root, old);
    dom.append_child(root, last);

    let new = dom.create_element("d");
    assert!(dom.replace_child(root, new, old));

    assert_eq!(dom.get(root).unwrap().children, vec![first, new, last]);
    assert_eq!(dom.get(new).unwrap().parent, Some(root));
    // Non-destructive: `old` keeps its identity, is detached (no parent),
    // and stays reattachable — see spec/architecture/primitives.md §4.1.
    assert_eq!(dom.get(old).unwrap().parent, None);
    let other_parent = dom.create_element("div");
    dom.append_child(root, other_parent);
    dom.append_child(other_parent, old);
    assert_eq!(dom.get(other_parent).unwrap().children, vec![old]);
}

#[test]
fn replace_child_with_itself_is_a_no_op() {
    let mut dom = Dom::new();
    let root = dom.root();
    let child = dom.create_element("div");
    dom.append_child(root, child);

    assert!(dom.replace_child(root, child, child));
    assert_eq!(dom.get(root).unwrap().children, vec![child]);
}

#[test]
fn replace_child_fails_when_old_child_is_not_actually_a_child() {
    let mut dom = Dom::new();
    let root = dom.root();
    let child = dom.create_element("div");
    let stranger = dom.create_element("span");
    dom.append_child(root, child);

    let new = dom.create_element("new");
    assert!(!dom.replace_child(root, new, stranger));
    assert_eq!(dom.get(root).unwrap().children, vec![child]);
}
