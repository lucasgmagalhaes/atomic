use dom::{DirtyFlags, Dom};

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

#[test]
fn attribute_reads_back_a_set_value_and_none_for_missing() {
    let mut dom = Dom::new();
    let div = dom.create_element("div");
    dom.set_attribute(div, "class", "box");

    assert_eq!(dom.attribute(div, "class"), Some("box"));
    assert_eq!(dom.attribute(div, "id"), None);
}

#[test]
fn append_text_merges_onto_an_existing_text_node() {
    let mut dom = Dom::new();
    let t = dom.create_text("hello ");
    dom.append_text(t, "world");
    assert!(matches!(dom.get(t).unwrap().data, dom::NodeData::Text(ref s) if s == "hello world"));
}

#[test]
fn value_defaults_to_empty_for_input_and_falls_back_to_text_for_textarea() {
    let mut dom = Dom::new();
    let input = dom.create_element("input");
    assert_eq!(dom.value(input), "");

    let textarea = dom.create_element("textarea");
    let text = dom.create_text("seeded");
    dom.append_child(textarea, text);
    assert_eq!(dom.value(textarea), "seeded");
}

#[test]
fn set_value_is_independent_of_text_content() {
    let mut dom = Dom::new();
    let input = dom.create_element("input");
    dom.set_value(input, "typed");

    assert_eq!(dom.value(input), "typed");
    assert_eq!(dom.text_content(input), "");
}

#[test]
fn set_attribute_value_seeds_the_value_property() {
    let mut dom = Dom::new();
    let input = dom.create_element("input");
    dom.set_attribute(input, "value", "seeded");

    assert_eq!(dom.value(input), "seeded");
}

#[test]
fn set_value_does_not_override_the_textarea_fallback_until_called() {
    let mut dom = Dom::new();
    let textarea = dom.create_element("textarea");
    let text = dom.create_text("initial");
    dom.append_child(textarea, text);
    assert_eq!(dom.value(textarea), "initial");

    dom.set_value(textarea, "edited");
    assert_eq!(dom.value(textarea), "edited");
    // The fallback only ever applied before `.value` was ever set - the
    // text node itself is untouched by `set_value`.
    assert_eq!(dom.text_content(textarea), "initial");
}

#[test]
fn focus_and_blur_track_active_element() {
    let mut dom = Dom::new();
    let root = dom.root();
    let input = dom.create_element("input");
    dom.append_child(root, input);

    assert_eq!(dom.active_element(), None);

    dom.focus(input);
    assert_eq!(dom.active_element(), Some(input));

    let result = dom.blur(input);
    assert_eq!(dom.active_element(), None);
    assert_eq!(result, Some(false));
}

#[test]
fn blur_is_a_no_op_for_a_node_that_is_not_the_focused_one() {
    let mut dom = Dom::new();
    let a = dom.create_element("input");
    let b = dom.create_element("input");

    dom.focus(a);
    let result = dom.blur(b);

    assert_eq!(dom.active_element(), Some(a));
    assert_eq!(result, None);
}

#[test]
fn blur_reports_whether_value_changed_since_focus() {
    let mut dom = Dom::new();
    let input = dom.create_element("input");

    dom.focus(input);
    dom.set_value(input, "typed");
    let changed = dom.blur(input);
    assert_eq!(changed, Some(true));

    dom.focus(input);
    let unchanged = dom.blur(input);
    assert_eq!(unchanged, Some(false));
}

#[test]
fn clear_focus_unfocuses_regardless_of_which_node_is_focused() {
    let mut dom = Dom::new();
    let input = dom.create_element("input");
    dom.focus(input);

    dom.clear_focus();

    assert_eq!(dom.active_element(), None);
}

#[test]
fn focus_on_a_nonexistent_node_is_a_no_op() {
    let mut dom = Dom::new();
    let input = dom.create_element("input");
    dom.remove(input);

    dom.focus(input);

    assert_eq!(dom.active_element(), None);
}

#[test]
fn removing_the_focused_node_clears_active_element() {
    let mut dom = Dom::new();
    let root = dom.root();
    let input = dom.create_element("input");
    dom.append_child(root, input);
    dom.focus(input);

    dom.remove(input);

    assert_eq!(dom.active_element(), None);
}

#[test]
fn tab_order_lists_inputs_and_textareas_in_document_order_and_skips_other_tags() {
    let mut dom = Dom::new();
    let root = dom.root();
    let a = dom.create_element("input");
    let div = dom.create_element("div");
    let b = dom.create_element("textarea");
    dom.append_child(root, a);
    dom.append_child(root, div);
    dom.append_child(root, b);

    assert_eq!(dom.tab_order(), vec![a, b]);
}

#[test]
fn tab_order_excludes_a_negative_tabindex() {
    let mut dom = Dom::new();
    let root = dom.root();
    let a = dom.create_element("input");
    let b = dom.create_element("input");
    dom.set_attribute(b, "tabindex", "-1");
    dom.append_child(root, a);
    dom.append_child(root, b);

    assert_eq!(dom.tab_order(), vec![a]);
}

#[test]
fn tab_order_puts_positive_tabindex_first_sorted_ascending_before_document_order_group() {
    let mut dom = Dom::new();
    let root = dom.root();
    let first_in_doc = dom.create_element("input");
    let tabindex_two = dom.create_element("input");
    dom.set_attribute(tabindex_two, "tabindex", "2");
    let tabindex_one = dom.create_element("input");
    dom.set_attribute(tabindex_one, "tabindex", "1");
    dom.append_child(root, first_in_doc);
    dom.append_child(root, tabindex_two);
    dom.append_child(root, tabindex_one);

    assert_eq!(
        dom.tab_order(),
        vec![tabindex_one, tabindex_two, first_in_doc]
    );
}

#[test]
fn tab_order_includes_a_non_input_element_with_an_explicit_tabindex() {
    let mut dom = Dom::new();
    let root = dom.root();
    let div = dom.create_element("div");
    dom.set_attribute(div, "tabindex", "0");
    dom.append_child(root, div);

    assert_eq!(dom.tab_order(), vec![div]);
}

#[test]
fn next_focus_target_cycles_forward_and_wraps() {
    let mut dom = Dom::new();
    let root = dom.root();
    let a = dom.create_element("input");
    let b = dom.create_element("input");
    dom.append_child(root, a);
    dom.append_child(root, b);

    assert_eq!(dom.next_focus_target(false), Some(a));
    dom.focus(a);
    assert_eq!(dom.next_focus_target(false), Some(b));
    dom.focus(b);
    assert_eq!(dom.next_focus_target(false), Some(a));
}

#[test]
fn next_focus_target_reverse_cycles_backward_and_wraps() {
    let mut dom = Dom::new();
    let root = dom.root();
    let a = dom.create_element("input");
    let b = dom.create_element("input");
    dom.append_child(root, a);
    dom.append_child(root, b);

    assert_eq!(dom.next_focus_target(true), Some(b));
    dom.focus(b);
    assert_eq!(dom.next_focus_target(true), Some(a));
    dom.focus(a);
    assert_eq!(dom.next_focus_target(true), Some(b));
}

#[test]
fn next_focus_target_is_none_when_the_tab_order_is_empty() {
    let dom = Dom::new();
    assert_eq!(dom.next_focus_target(false), None);
    assert_eq!(dom.next_focus_target(true), None);
}

#[test]
fn serialize_children_mixes_text_and_elements_with_escaping() {
    let mut dom = Dom::new();
    let div = dom.create_element("div");
    let t1 = dom.create_text("a < b & c > d");
    let span = dom.create_element("span");
    dom.set_attribute(span, "title", "quote \" amp &");
    let t2 = dom.create_text("inner");
    dom.append_child(div, t1);
    dom.append_child(div, span);
    dom.append_child(span, t2);

    assert_eq!(
        dom.serialize_children(div),
        "a &lt; b &amp; c &gt; d<span title=\"quote &quot; amp &amp;\">inner</span>"
    );
}

#[test]
fn serialize_children_handles_nested_subtree_round_trip() {
    let mut dom = Dom::new();
    let ul = dom.create_element("ul");
    let li = dom.create_element("li");
    let text = dom.create_text("item");
    dom.append_child(ul, li);
    dom.append_child(li, text);

    assert_eq!(dom.serialize_children(ul), "<li>item</li>");
}

#[test]
fn serialize_node_includes_own_tag_and_sorted_attributes() {
    let mut dom = Dom::new();
    let div = dom.create_element("div");
    dom.set_attribute(div, "class", "box");
    dom.set_attribute(div, "id", "main");
    let text = dom.create_text("hi");
    dom.append_child(div, text);

    assert_eq!(
        dom.serialize_node(div),
        "<div class=\"box\" id=\"main\">hi</div>"
    );
}

#[test]
fn serialize_node_equals_wrapped_serialize_children() {
    let mut dom = Dom::new();
    let p = dom.create_element("p");
    dom.set_attribute(p, "id", "para");
    let text = dom.create_text("content");
    dom.append_child(p, text);

    let expected = format!("<p id=\"para\">{}</p>", dom.serialize_children(p));
    assert_eq!(dom.serialize_node(p), expected);
}

#[test]
fn serialize_node_on_comment_and_text_matches_their_markup() {
    let mut dom = Dom::new();
    let comment = dom.create_comment("note");
    let text = dom.create_text("a & b");

    assert_eq!(dom.serialize_node(comment), "<!--note-->");
    assert_eq!(dom.serialize_node(text), "a &amp; b");
}

#[test]
fn adopt_deep_clones_a_subtree_into_a_different_dom() {
    let mut source = Dom::new();
    let src_root = source.create_element("div");
    source.set_attribute(src_root, "class", "widget");
    let src_child = source.create_element("span");
    source.set_attribute(src_child, "data-x", "1");
    let src_text = source.create_text("hello");
    source.append_child(src_root, src_child);
    source.append_child(src_child, src_text);

    let mut dest = Dom::new();
    let dest_root = dest.root();
    let cloned = dest.adopt(&source, src_root);
    dest.append_child(dest_root, cloned);

    assert_eq!(dest.serialize_node(cloned), source.serialize_node(src_root));
    assert_eq!(dest.text_content(cloned), "hello");
    assert_eq!(dest.attribute(cloned, "class"), Some("widget"));

    // Mutating the clone must not affect the original source subtree.
    dest.set_attribute(cloned, "class", "changed");
    assert_eq!(dest.attribute(cloned, "class"), Some("changed"));
    assert_eq!(source.attribute(src_root, "class"), Some("widget"));
}

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
