use js_runtime::{Context, Runtime};

#[test]
fn nodevalue_returns_text_data_for_text_nodes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let text = d.create_text("hello");
    d.append_child(body, text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "document.querySelector('body').firstChild.nodeValue",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "hello");
}

#[test]
fn nodevalue_returns_null_for_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "document.querySelector('body').nodeValue === null",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn nodevalue_setter_updates_text_node() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let text = d.create_text("old");
    d.append_child(body, text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const t = document.querySelector('body').firstChild; t.nodeValue = 'new'; return t.nodeValue; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "new");
}

#[test]
fn nodevalue_setter_is_noop_on_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { document.querySelector('body').nodeValue = 'x'; return document.querySelector('body').nodeValue; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "null");
}

#[test]
fn nodevalue_returns_text_for_comment() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let comment = d.create_comment("a comment");
    d.append_child(body, comment);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "document.querySelector('body').firstChild.nodeValue",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "a comment");
}

#[test]
fn nodevalue_setter_updates_comment() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let comment = d.create_comment("old");
    d.append_child(body, comment);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const c = document.querySelector('body').firstChild; c.nodeValue = 'updated'; return c.nodeValue; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "updated");
}

#[test]
fn nodevalue_is_null_for_document_fragment() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "document.createDocumentFragment().nodeValue === null",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}
