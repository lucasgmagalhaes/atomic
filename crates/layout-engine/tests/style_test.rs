//! Inline `style="..."` HTML attribute cascade coverage — inline style must
//! win over any stylesheet rule regardless of selector specificity (CSS
//! Cascading and Inheritance §6.4.1).

use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, Color, Length};

#[test]
fn inline_style_attribute_is_applied() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);
    d.set_attribute(div, "style", "width: 300px; height: 150px;");

    let sheet = parse_stylesheet("");
    let tree = build_box_tree(&d, div, &sheet).unwrap();

    assert_eq!(tree.style.width, Length::Px(300.0));
    assert_eq!(tree.style.height, Length::Px(150.0));
}

#[test]
fn inline_style_wins_over_a_matching_stylesheet_rule_of_any_specificity() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);
    d.set_attribute(div, "id", "target");
    d.set_attribute(div, "style", "background-color: #336699;");

    // `#target` has higher specificity than a type selector, yet the
    // inline declaration must still win.
    let sheet = parse_stylesheet("#target { background-color: red; }");
    let tree = build_box_tree(&d, div, &sheet).unwrap();

    assert_eq!(
        tree.style.background_color,
        Color {
            r: 0x33,
            g: 0x66,
            b: 0x99,
            a: 255
        }
    );
}

#[test]
fn absent_style_attribute_leaves_stylesheet_cascade_untouched() {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let sheet = parse_stylesheet("div { width: 100px; }");
    let tree = build_box_tree(&d, div, &sheet).unwrap();

    assert_eq!(tree.style.width, Length::Px(100.0));
}
