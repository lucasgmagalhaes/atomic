use js_runtime::{Context, Runtime};

#[test]
fn inner_html_read_side_serializes_children() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    d.append_child(root, parent);
    let child = d.create_element("span");
    d.set_attribute(child, "class", "greeting");
    d.append_child(parent, child);
    d.set_text_content(child, "hi & bye");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval("document.getElementById('parent').innerHTML", "<test>")
        .unwrap();
    assert_eq!(result, "<span class=\"greeting\">hi &amp; bye</span>");
}

#[test]
fn inner_html_write_side_replaces_children_and_evicts_old_ones() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    d.append_child(root, parent);
    let old_child = d.create_element("span");
    d.set_attribute(old_child, "id", "old");
    d.append_child(parent, old_child);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const parent = document.getElementById('parent'); \
                let oldSeen = false; \
                document.getElementById('old').addEventListener('click', () => { oldSeen = true; }); \
                parent.innerHTML = '<p id=\"new\">hello <b>world</b></p>'; \
                const newP = document.getElementById('new'); \
                return `${document.getElementById('old')},${newP.nodeName},${newP.textContent},${parent.innerHTML}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result,
        "null,P,hello world,<p id=\"new\">hello <b>world</b></p>"
    );
}

#[test]
fn outer_html_read_side_includes_own_tag_and_write_side_replaces_the_node() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    d.append_child(root, parent);
    let target = d.create_element("span");
    d.set_attribute(target, "id", "target");
    d.append_child(parent, target);
    d.set_text_content(target, "old");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let read = ctx
        .eval("document.getElementById('target').outerHTML", "<test>")
        .unwrap();
    assert_eq!(read, "<span id=\"target\">old</span>");

    let result = ctx
        .eval(
            "(() => { \
                document.getElementById('target').outerHTML = '<em id=\"new\">fresh</em>'; \
                const replaced = document.getElementById('target'); \
                const created = document.getElementById('new'); \
                return `${replaced},${created.nodeName},${document.getElementById('parent').innerHTML}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "null,EM,<em id=\"new\">fresh</em>");
}

#[test]
fn html_setters_reject_oversized_input_and_coerce_non_strings() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    d.append_child(root, parent);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('parent'); \
                el.innerHTML = 123; \
                const coerced = el.textContent; \
                let sizeRejected = false; \
                try { el.outerHTML = 'x'.repeat(200000); } catch (_) { sizeRejected = true; } \
                return `${coerced},${sizeRejected}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "123,true");
}
