use js_runtime::{Context, Runtime};

#[test]
fn get_elements_by_tag_name_returns_html_collection_with_item_and_named_item() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let div1 = d.create_element("div");
    d.set_attribute(div1, "id", "foo");
    d.append_child(body, div1);
    let div2 = d.create_element("div");
    d.set_attribute(div2, "name", "bar");
    d.append_child(body, div2);
    let span = d.create_element("span");
    d.append_child(body, span);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const c = document.getElementsByTagName('div'); return `${c.length},${c.item(0).id},${c.namedItem('foo').id},${c.namedItem('bar').name}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,foo,foo,bar");
}

#[test]
fn query_selector_all_returns_nodelist_with_item() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let d1 = d.create_element("div");
    d.append_child(body, d1);
    let d2 = d.create_element("div");
    d.append_child(body, d2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const nl = document.querySelectorAll('div'); return `${nl.length},${nl.item(0) === nl[0]},${nl.item(5)}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,true,null");
}

#[test]
fn get_elements_by_class_name_returns_html_collection() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let d1 = d.create_element("div");
    d.set_attribute(d1, "class", "x y");
    d.append_child(body, d1);
    let d2 = d.create_element("div");
    d.set_attribute(d2, "class", "x");
    d.append_child(body, d2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const c = document.getElementsByClassName('x'); return `${c.length},${c.item(0).getAttribute('class')}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,x y");
}

#[test]
fn document_forms_is_html_collection_with_named_item() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let f1 = d.create_element("form");
    d.set_attribute(f1, "name", "login");
    d.append_child(body, f1);
    let f2 = d.create_element("form");
    d.set_attribute(f2, "name", "search");
    d.append_child(body, f2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const forms = document.forms; return `${forms.length},${forms.namedItem('search').name},${forms.item(0).name}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,search,login");
}

#[test]
fn childnodes_returns_nodelist_with_item_and_length() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let d1 = d.create_element("div");
    d.append_child(body, d1);
    let text = d.create_text("hello");
    d.append_child(body, text);
    let d2 = d.create_element("div");
    d.append_child(body, d2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const cn = document.body.childNodes; return `${cn.length},${cn.item(0) === cn[0]},${cn.item(1).nodeValue},${cn.item(99)}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "3,true,hello,null");
}

#[test]
fn childnodes_includes_text_nodes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let t1 = d.create_text("a");
    d.append_child(body, t1);
    let t2 = d.create_text("b");
    d.append_child(body, t2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const cn = document.body.childNodes; return `${cn.length},${cn.item(0).nodeValue},${cn.item(1).nodeValue}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,a,b");
}

#[test]
fn form_elements_returns_controls_as_html_collection() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let form = d.create_element("form");
    d.set_attribute(form, "name", "signup");
    d.append_child(body, form);
    let input = d.create_element("input");
    d.set_attribute(input, "name", "user");
    d.append_child(form, input);
    let select = d.create_element("select");
    d.set_attribute(select, "name", "color");
    d.append_child(form, select);
    let button = d.create_element("button");
    d.append_child(form, button);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const f = document.forms.namedItem('signup'); return `${f instanceof HTMLFormElement},${f.elements.length},${f.elements.namedItem('user').name},${f.submit !== undefined},${f.reset !== undefined}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,3,user,true,true");
}
