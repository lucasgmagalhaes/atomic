use dom::Dom;

#[test]
fn create_and_append_sets_parent_and_children() {
    let mut dom = Dom::new();
    let root = dom.root();
    let div = dom.create_element("div");

    dom.append_child(root, div);

    assert_eq!(dom.get(div).unwrap().parent, Some(root));
    assert_eq!(dom.get(root).unwrap().children, vec![div]);
}

#[test]
fn reparent_removes_child_from_old_parent() {
    let mut dom = Dom::new();
    let parent_a = dom.create_element("div");
    let parent_b = dom.create_element("section");
    let child = dom.create_element("span");

    dom.append_child(parent_a, child);
    dom.append_child(parent_b, child);

    assert!(dom.get(parent_a).unwrap().children.is_empty());
    assert_eq!(dom.get(parent_b).unwrap().children, vec![child]);
    assert_eq!(dom.get(child).unwrap().parent, Some(parent_b));
}

#[test]
fn remove_frees_slot_and_invalidates_old_id_via_generation() {
    let mut dom = Dom::new();
    let root = dom.root();
    let div = dom.create_element("div");
    dom.append_child(root, div);

    dom.remove(div);

    assert!(dom.get(div).is_none());
    assert!(dom.get(root).unwrap().children.is_empty());

    // The freed slot gets reused with a bumped generation, so the old id
    // must never alias the new node.
    let reused = dom.create_text("hi");
    assert_ne!(dom.get(reused).unwrap().id, div);
}

#[test]
fn remove_subtree_removes_all_descendants() {
    let mut dom = Dom::new();
    let root = dom.root();
    let parent = dom.create_element("ul");
    let child1 = dom.create_element("li");
    let child2 = dom.create_element("li");
    dom.append_child(root, parent);
    dom.append_child(parent, child1);
    dom.append_child(parent, child2);

    dom.remove(parent);

    assert!(dom.get(parent).is_none());
    assert!(dom.get(child1).is_none());
    assert!(dom.get(child2).is_none());
    assert!(dom.get(root).unwrap().children.is_empty());
}

#[test]
fn find_by_id_locates_nested_element() {
    let mut dom = Dom::new();
    let root = dom.root();
    let wrapper = dom.create_element("div");
    let target = dom.create_element("span");
    dom.append_child(root, wrapper);
    dom.append_child(wrapper, target);
    dom.set_attribute(target, "id", "target");

    assert_eq!(dom.find_by_id("target"), Some(target));
    assert_eq!(dom.find_by_id("missing"), None);
}

#[test]
fn text_content_concatenates_descendant_text_nodes() {
    let mut dom = Dom::new();
    let root = dom.root();
    let p = dom.create_element("p");
    let t1 = dom.create_text("hello ");
    let span = dom.create_element("span");
    let t2 = dom.create_text("world");
    dom.append_child(root, p);
    dom.append_child(p, t1);
    dom.append_child(p, span);
    dom.append_child(span, t2);

    assert_eq!(dom.text_content(p), "hello world");
}

#[test]
fn set_text_content_replaces_all_children() {
    let mut dom = Dom::new();
    let root = dom.root();
    let p = dom.create_element("p");
    let old_child = dom.create_element("span");
    dom.append_child(root, p);
    dom.append_child(p, old_child);

    dom.set_text_content(p, "replaced");

    assert!(dom.get(old_child).is_none());
    assert_eq!(dom.text_content(p), "replaced");
    assert_eq!(dom.get(p).unwrap().children.len(), 1);
}

#[test]
fn create_comment_produces_a_comment_node() {
    let mut dom = Dom::new();
    let c = dom.create_comment("a comment");
    assert!(matches!(dom.get(c).unwrap().data, dom::NodeData::Comment(ref s) if s == "a comment"));
}

#[test]
fn remove_from_parent_detaches_without_freeing() {
    let mut dom = Dom::new();
    let root = dom.root();
    let div = dom.create_element("div");
    dom.append_child(root, div);

    dom.remove_from_parent(div);

    assert!(dom.get(root).unwrap().children.is_empty());
    // Still alive - unlike remove(), which frees the slot.
    assert!(dom.get(div).is_some());
    assert_eq!(dom.get(div).unwrap().parent, None);
}

#[test]
fn insert_before_places_new_node_ahead_of_sibling() {
    let mut dom = Dom::new();
    let root = dom.root();
    let a = dom.create_element("a");
    let b = dom.create_element("b");
    dom.append_child(root, a);
    dom.append_child(root, b);

    let c = dom.create_element("c");
    dom.insert_before(b, c);

    assert_eq!(dom.get(root).unwrap().children, vec![a, c, b]);
}
