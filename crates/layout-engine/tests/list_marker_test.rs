use css::parse_stylesheet;
use dom::Dom;
use layout_engine::build_box_tree;

fn ol_with_two_items() -> (Dom, dom::NodeId) {
    let mut d = Dom::new();
    let root = d.root();
    let ol = d.create_element("ol");
    d.append_child(root, ol);
    for label in ["first", "second"] {
        let li = d.create_element("li");
        let text = d.create_text(label);
        d.append_child(li, text);
        d.append_child(ol, li);
    }
    (d, ol)
}

#[test]
fn ol_items_default_to_decimal_markers_starting_at_one() {
    let (d, ol) = ol_with_two_items();
    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, ol, &sheet).unwrap();

    assert_eq!(tree.children.len(), 2);
    assert_eq!(
        tree.children[0].children[0].text.as_deref(),
        Some("1. first")
    );
    assert_eq!(
        tree.children[1].children[0].text.as_deref(),
        Some("2. second")
    );
}

#[test]
fn ul_items_default_to_a_disc_marker() {
    let mut d = Dom::new();
    let root = d.root();
    let ul = d.create_element("ul");
    let li = d.create_element("li");
    let text = d.create_text("item");
    d.append_child(li, text);
    d.append_child(ul, li);
    d.append_child(root, ul);

    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, ul, &sheet).unwrap();

    assert_eq!(
        tree.children[0].children[0].text.as_deref(),
        Some("\u{2022} item")
    );
}

#[test]
fn list_style_type_none_suppresses_the_marker_but_not_the_counter() {
    let (d, ol) = ol_with_two_items();
    let sheet = parse_stylesheet("li:first-child { list-style-type: none; }");
    let tree = build_box_tree(&d, ol, &sheet).unwrap();

    assert_eq!(tree.children[0].children[0].text.as_deref(), Some("first"));
    // The second item's ordinal still counts the first, suppressed one.
    assert_eq!(
        tree.children[1].children[0].text.as_deref(),
        Some("2. second")
    );
}

#[test]
fn list_style_type_can_be_overridden_per_element() {
    let (d, ol) = ol_with_two_items();
    let sheet = parse_stylesheet("ol { list-style-type: square; }");
    let tree = build_box_tree(&d, ol, &sheet).unwrap();

    assert_eq!(
        tree.children[0].children[0].text.as_deref(),
        Some("\u{25AA} first")
    );
}

#[test]
fn nested_ordered_list_restarts_its_own_counter() {
    let mut d = Dom::new();
    let root = d.root();
    let ol = d.create_element("ol");
    let li = d.create_element("li");
    let nested_ol = d.create_element("ol");
    let nested_li = d.create_element("li");
    let nested_text = d.create_text("nested");
    d.append_child(nested_li, nested_text);
    d.append_child(nested_ol, nested_li);
    d.append_child(li, nested_ol);
    d.append_child(ol, li);
    d.append_child(root, ol);

    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, ol, &sheet).unwrap();

    // tree.children[0] is the outer <li>; its own children hold the
    // nested <ol> box (no leading text child to merge a marker into,
    // since the <li>'s only child is itself block-level - the outer
    // <li> gets a standalone leading marker box instead).
    let outer_li = &tree.children[0];
    assert_eq!(outer_li.children[0].text.as_deref(), Some("1. "));
    let nested_ol_box = &outer_li.children[1];
    assert_eq!(
        nested_ol_box.children[0].children[0].text.as_deref(),
        Some("1. nested")
    );
}
