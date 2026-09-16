use std::collections::HashMap;

use js_runtime::{Context, Runtime};

#[test]
fn get_computed_style_reflects_host_pushed_values() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let el = d.create_element("div");
    d.set_attribute(el, "id", "box");
    d.append_child(root, el);

    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);

    let mut properties = HashMap::new();
    properties.insert("display".to_string(), "block".to_string());
    properties.insert("color".to_string(), "rgba(255, 0, 0, 1)".to_string());
    let mut styles = HashMap::new();
    styles.insert(el, properties);
    ctx.set_computed_styles(styles);

    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('box'); \
                const style = getComputedStyle(el); \
                return `${style.display},${style.color},${style.getPropertyValue('color')},${style.cssText.includes('display: block')}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "block,rgba(255, 0, 0, 1),rgba(255, 0, 0, 1),true");
}

#[test]
fn get_computed_style_on_an_unlaid_out_node_is_empty() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let el = d.create_element("div");
    d.set_attribute(el, "id", "box");
    d.append_child(root, el);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const style = getComputedStyle(document.getElementById('box')); \
                return `${style.cssText},${style.getPropertyValue('display')}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, ",");
}
