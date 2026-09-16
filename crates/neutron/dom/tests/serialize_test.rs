use dom::Dom;

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
