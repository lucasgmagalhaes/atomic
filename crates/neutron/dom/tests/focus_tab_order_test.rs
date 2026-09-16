use dom::Dom;

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
