use js_runtime::{Context, Runtime};

#[test]
fn element_instanceof_node_element_html_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('div'); return `${el instanceof Node},${el instanceof Element},${el instanceof HTMLElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true");
}

#[test]
fn text_node_instanceof_node_only() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);
    let text = d.create_text("hello");
    d.append_child(div, text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const t = document.querySelector('div').firstChild; return `${t.nodeType},${t instanceof Node},${t instanceof Element},${t instanceof HTMLElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "3,true,false,false");
}

#[test]
fn input_element_instanceof_html_input_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('input'); return `${el instanceof Node},${el instanceof Element},${el instanceof HTMLElement},${el instanceof HTMLInputElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true,true");
}

#[test]
fn button_element_instanceof_html_button_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let btn = d.create_element("button");
    d.append_child(root, btn);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('button'); return `${el instanceof HTMLButtonElement},${el instanceof HTMLInputElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false");
}

#[test]
fn anchor_element_instanceof_html_anchor_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let a = d.create_element("a");
    d.append_child(root, a);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('a'); return `${el instanceof HTMLAnchorElement},${el instanceof HTMLInputElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false");
}

#[test]
fn generic_element_not_instanceof_input() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('div'); return `${el instanceof HTMLInputElement},${el instanceof HTMLButtonElement},${el instanceof HTMLAnchorElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false,false,false");
}

#[test]
fn node_constructor_exists_and_prototype_chain() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { return `${typeof Node},${typeof Element},${typeof HTMLElement},${typeof HTMLInputElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "function,function,function,function");
}

#[test]
fn cloned_element_preserves_class_hierarchy() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const orig = document.querySelector('input'); const clone = orig.cloneNode(true); return `${clone instanceof HTMLInputElement},${clone instanceof HTMLElement},${clone instanceof Element},${clone instanceof Node}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true,true");
}
