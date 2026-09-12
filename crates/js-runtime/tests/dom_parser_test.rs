use js_runtime::{Context, Runtime};

#[test]
fn dom_parser_parses_html_into_a_queryable_fragment() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const frag = new DOMParser().parseFromString('<div id=\"a\"><span>hi</span></div><p>there</p>', 'text/html'); return `${frag.nodeType === 11},${frag.childNodes.length},${frag.querySelector('#a').textContent},${frag.querySelector('p').tagName}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,2,hi,P");
}

#[test]
fn dom_parser_fragment_can_be_adopted_into_the_document() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const frag = new DOMParser().parseFromString('<b>bold</b>', 'text/html'); document.body.appendChild(frag.firstChild); return document.body.innerHTML; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "<b>bold</b>");
}

#[test]
fn xml_serializer_serializes_a_node_back_to_html() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let div = d.create_element("div");
    d.set_attribute(div, "class", "box");
    d.append_child(body, div);
    let text = d.create_text("hello");
    d.append_child(div, text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('div.box'); const out = new XMLSerializer().serializeToString(el); return `${out === '<div class=\"box\">hello</div>'},${typeof XMLSerializer}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,function");
}
