use js_runtime::{Context, Runtime};

#[test]
fn document_url_returns_the_set_url() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let _body = d.create_element("body");
    d.append_child(root, _body);

    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://example.com/page?foo=bar#section");
    let result = ctx.eval("document.URL", "<test>").unwrap();
    assert_eq!(result, "https://example.com/page?foo=bar#section");
}

#[test]
fn document_base_uri_matches_url() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let _body = d.create_element("body");
    d.append_child(root, _body);

    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://example.com/index.html");
    let result = ctx.eval("document.baseURI", "<test>").unwrap();
    assert_eq!(result, "https://example.com/index.html");
}

#[test]
fn document_url_and_baseuri_are_empty_when_unset() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let _body = d.create_element("body");
    d.append_child(root, _body);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let url = ctx.eval("document.URL", "<test>").unwrap();
    let base = ctx.eval("document.baseURI", "<test>").unwrap();
    assert_eq!(url, "");
    assert_eq!(base, "");
}

#[test]
fn document_forms_returns_all_form_elements() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let f1 = d.create_element("form");
    d.append_child(body, f1);
    let f2 = d.create_element("form");
    d.append_child(body, f2);
    let _div = d.create_element("div");
    d.append_child(body, _div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("document.forms.length", "<test>").unwrap();
    assert_eq!(result, "2");
}

#[test]
fn document_images_returns_all_img_elements() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let img1 = d.create_element("img");
    d.append_child(body, img1);
    let img2 = d.create_element("img");
    d.append_child(body, img2);
    let _div = d.create_element("div");
    d.append_child(body, _div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("document.images.length", "<test>").unwrap();
    assert_eq!(result, "2");
}

#[test]
fn document_scripts_returns_all_script_elements() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let s1 = d.create_element("script");
    d.append_child(body, s1);
    let _div = d.create_element("div");
    d.append_child(body, _div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("document.scripts.length", "<test>").unwrap();
    assert_eq!(result, "1");
}

#[test]
fn document_links_returns_href_anchors_and_areas() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let a1 = d.create_element("a");
    d.set_attribute(a1, "href", "/page1");
    d.append_child(body, a1);
    let a2 = d.create_element("a");
    d.append_child(body, a2);
    let area = d.create_element("area");
    d.set_attribute(area, "href", "/area");
    d.append_child(body, area);
    let area2 = d.create_element("area");
    d.append_child(body, area2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("document.links.length", "<test>").unwrap();
    assert_eq!(result, "2");
}

#[test]
fn document_forms_collection_items_are_nodes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let f1 = d.create_element("form");
    d.set_attribute(f1, "id", "myform");
    d.append_child(body, f1);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("document.forms[0].id", "<test>").unwrap();
    assert_eq!(result, "myform");
}
