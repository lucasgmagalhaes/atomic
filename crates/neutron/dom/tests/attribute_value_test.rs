use dom::Dom;

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
