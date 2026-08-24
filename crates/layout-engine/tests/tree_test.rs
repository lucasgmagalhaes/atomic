use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, Display, Length};

#[test]
fn builds_box_for_matching_element() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; }");
    let tree = build_box_tree(&d, div, &sheet).unwrap();

    assert_eq!(tree.node, div);
    assert_eq!(tree.style.width, Length::Px(100.0));
}

#[test]
fn builds_nested_boxes_for_element_children() {
    let mut d = Dom::new();
    let root = d.root();
    let outer = d.create_element("div");
    let inner = d.create_element("span");
    d.append_child(root, outer);
    d.append_child(outer, inner);

    let sheet = parse_stylesheet("span { width: 50px; }");
    let tree = build_box_tree(&d, outer, &sheet).unwrap();

    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].node, inner);
    assert_eq!(tree.children[0].style.width, Length::Px(50.0));
}

#[test]
fn text_nodes_produce_a_leaf_box_carrying_their_string() {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("hello");
    d.append_child(root, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, p, &sheet).unwrap();

    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].text.as_deref(), Some("hello"));
    assert!(tree.children[0].children.is_empty());
}

#[test]
fn whitespace_only_text_nodes_produce_no_box() {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("   \n  ");
    d.append_child(root, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, p, &sheet).unwrap();

    assert!(tree.children.is_empty());
}

#[test]
fn display_none_prunes_element_and_its_subtree() {
    let mut d = Dom::new();
    let root = d.root();
    let outer = d.create_element("div");
    let hidden = d.create_element("span");
    let grandchild = d.create_element("em");
    d.append_child(root, outer);
    d.append_child(outer, hidden);
    d.append_child(hidden, grandchild);

    let sheet = parse_stylesheet("span { display: none; }");
    let tree = build_box_tree(&d, outer, &sheet).unwrap();

    assert!(tree.children.is_empty());
}

#[test]
fn id_and_class_selectors_apply_through_the_tree() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);
    d.set_attribute(div, "id", "main");
    d.set_attribute(div, "class", "box highlighted");

    let sheet = parse_stylesheet("#main { width: 10px; } .highlighted { height: 20px; }");
    let tree = build_box_tree(&d, div, &sheet).unwrap();

    assert_eq!(tree.style.width, Length::Px(10.0));
    assert_eq!(tree.style.height, Length::Px(20.0));
}

#[test]
fn descendant_selector_matches_through_dom_ancestry() {
    let mut d = Dom::new();
    let root = d.root();
    let outer = d.create_element("div");
    let inner = d.create_element("span");
    d.append_child(root, outer);
    d.append_child(outer, inner);
    d.set_attribute(outer, "id", "container");

    let sheet = parse_stylesheet("#container span { width: 33px; }");
    let tree = build_box_tree(&d, outer, &sheet).unwrap();

    assert_eq!(tree.children[0].style.width, Length::Px(33.0));
}

#[test]
fn dimensions_default_to_zero_before_layout_runs() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, div, &sheet).unwrap();

    assert_eq!(tree.dimensions.width, 0.0);
    assert_eq!(tree.dimensions.height, 0.0);
    assert_eq!(tree.style.display, Display::Block);
}

#[test]
fn style_and_script_elements_are_never_rendered_even_with_explicit_display_block() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("html");
    d.append_child(root, container);
    let style_el = d.create_element("style");
    let text = d.create_text("div { color: red; }");
    d.append_child(style_el, text);
    let script_el = d.create_element("script");
    let script_text = d.create_text("doSomething();");
    d.append_child(script_el, script_text);
    d.append_child(container, style_el);
    d.append_child(container, script_el);

    // A page's own stylesheet trying to force these visible must not
    // override the unconditional UA-level "never rendered" rule.
    let sheet = parse_stylesheet("style, script { display: block; width: 100px; height: 100px; }");
    let tree = build_box_tree(&d, container, &sheet).unwrap();

    assert!(tree.children.is_empty(), "style/script must not produce a box");
}

#[test]
fn a_style_element_placed_before_content_does_not_push_it_down() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("html");
    d.append_child(root, container);
    let style_el = d.create_element("style");
    let style_text = d.create_text("#box { width: 10px; height: 10px; }");
    d.append_child(style_el, style_text);
    let div = d.create_element("div");
    d.set_attribute(div, "id", "box");
    d.append_child(container, style_el);
    d.append_child(container, div);

    let sheet = parse_stylesheet("#box { width: 10px; height: 10px; }");
    let tree = build_box_tree(&d, container, &sheet).unwrap();

    assert_eq!(tree.children.len(), 1, "the <style> element must not produce a sibling box");
    assert_eq!(tree.children[0].node, div);
}

#[test]
fn head_metadata_elements_are_pruned_from_the_box_tree() {
    let mut d = Dom::new();
    let root = d.root();
    let container = d.create_element("html");
    d.append_child(root, container);
    let head = d.create_element("head");
    let title = d.create_element("title");
    let title_text = d.create_text("Page Title");
    d.append_child(title, title_text);
    let meta = d.create_element("meta");
    d.append_child(head, title);
    d.append_child(head, meta);
    let body = d.create_element("body");
    d.append_child(container, head);
    d.append_child(container, body);

    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, container, &sheet).unwrap();

    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].node, body);
}
