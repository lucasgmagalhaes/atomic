use js_runtime::{Context, Runtime};

#[test]
fn normalize_merges_adjacent_text_nodes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);
    let t1 = d.create_text("hello");
    let t2 = d.create_text(" world");
    d.append_child(div, t1);
    d.append_child(div, t2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const div = document.querySelector('div'); const before = div.childNodes.length; div.normalize(); return `${before},${div.textContent},${div.childNodes.length}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,hello world,1");
}

#[test]
fn normalize_removes_empty_text_nodes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);
    let t1 = d.create_text("a");
    let empty = d.create_text("");
    let t2 = d.create_text("b");
    d.append_child(div, t1);
    d.append_child(div, empty);
    d.append_child(div, t2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const div = document.querySelector('div'); const before = div.childNodes.length; div.normalize(); return `${before},${div.textContent},${div.childNodes.length}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "3,ab,1");
}

#[test]
fn normalize_does_not_merge_text_with_element_neighbor() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);
    let t1 = d.create_text("hello");
    let span = d.create_element("span");
    let t2 = d.create_text(" world");
    d.append_child(div, t1);
    d.append_child(div, span);
    d.append_child(div, t2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const div = document.querySelector('div'); div.normalize(); return `${div.childNodes.length}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "3");
}

#[test]
fn create_document_fragment_is_empty_and_has_correct_node_type() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let result = ctx
        .eval(
            "(() => { const frag = document.createDocumentFragment(); return `${frag.nodeType},${frag.nodeName},${frag.childNodes.length}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "11,#document-fragment,0");
}

#[test]
fn document_fragment_can_hold_children() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let result = ctx
        .eval(
            "(() => { const frag = document.createDocumentFragment(); const div = document.createElement('div'); const span = document.createElement('span'); frag.appendChild(div); frag.appendChild(span); return `${frag.childNodes.length},${frag.firstChild.nodeName},${frag.lastChild.nodeName}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,DIV,SPAN");
}

#[test]
fn import_node_deep_clones_a_node() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.set_attribute(div, "id", "src");
    d.append_child(root, div);
    let child = d.create_element("span");
    d.append_child(div, child);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const src = document.getElementById('src'); const clone = document.importNode(src, true); return `${clone.id},${clone.nodeName},${clone.childNodes.length},${clone !== src}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "src,DIV,1,true");
}

#[test]
fn import_node_shallow_omits_children() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.set_attribute(div, "id", "src");
    d.append_child(root, div);
    let child = d.create_element("span");
    d.append_child(div, child);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const src = document.getElementById('src'); const clone = document.importNode(src, false); return `${clone.childNodes.length}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0");
}

#[test]
fn adopt_node_returns_same_node_in_single_document() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.set_attribute(div, "id", "mydiv");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const node = document.getElementById('mydiv'); const adopted = document.adoptNode(node); return `${adopted === node},${adopted.id}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,mydiv");
}
