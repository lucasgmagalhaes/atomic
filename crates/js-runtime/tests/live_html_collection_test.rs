//! Real *live* `HTMLCollection`: `getElementsByTagName`/
//! `getElementsByClassName` re-run their query on every access instead of
//! snapshotting once. Closes `spec/matrix/dom.md`'s "Collections" line
//! (liveness).

use js_runtime::{Context, Runtime};

fn dom_with_body() -> (dom::Dom, dom::NodeId) {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    (d, body)
}

#[test]
fn getelementsbytagname_reflects_a_node_appended_after_the_collection_was_captured() {
    let (mut d, body) = dom_with_body();
    let div1 = d.create_element("div");
    d.append_child(body, div1);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const c = document.getElementsByTagName('div'); \
                const before = c.length; \
                const el = document.createElement('div'); \
                el.id = 'added'; \
                document.body.appendChild(el); \
                return `${before},${c.length},${c.item(1).id}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result, "1,2,added",
        "a real live collection should see the appended div without re-calling getElementsByTagName"
    );
}

#[test]
fn getelementsbytagname_reflects_a_node_removed_after_the_collection_was_captured() {
    let (mut d, body) = dom_with_body();
    let div1 = d.create_element("div");
    d.set_attribute(div1, "id", "keep");
    d.append_child(body, div1);
    let div2 = d.create_element("div");
    d.set_attribute(div2, "id", "gone");
    d.append_child(body, div2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const c = document.getElementsByTagName('div'); \
                const before = c.length; \
                document.getElementById('gone').remove(); \
                return `${before},${c.length},${c.item(0).id}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result, "2,1,keep",
        "a real live collection should see the real removal without re-calling getElementsByTagName"
    );
}

#[test]
fn getelementsbyclassname_is_also_real_and_live() {
    let (mut d, body) = dom_with_body();
    let div1 = d.create_element("div");
    d.set_attribute(div1, "class", "tag");
    d.append_child(body, div1);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const c = document.getElementsByClassName('tag'); \
                const before = c.length; \
                const el = document.createElement('span'); \
                el.className = 'tag'; \
                document.body.appendChild(el); \
                return `${before},${c.length}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1,2");
}

#[test]
fn indexed_access_through_the_proxy_returns_a_real_node_with_live_methods() {
    let (mut d, body) = dom_with_body();
    let div1 = d.create_element("div");
    d.append_child(body, div1);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const c = document.getElementsByTagName('div'); \
                c[0].textContent = 'hi'; \
                return c[0].textContent; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "hi");
}

#[test]
fn document_forms_is_also_real_and_live() {
    let (mut d, body) = dom_with_body();
    let form1 = d.create_element("form");
    d.set_attribute(form1, "id", "f1");
    d.append_child(body, form1);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const c = document.forms; \
                const before = c.length; \
                const el = document.createElement('form'); \
                el.id = 'f2'; \
                document.body.appendChild(el); \
                return `${before},${c.length},${c.namedItem('f2').id}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1,2,f2");
}

#[test]
fn document_images_is_also_real_and_live() {
    let (mut d, body) = dom_with_body();
    let img1 = d.create_element("img");
    d.append_child(body, img1);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const c = document.images; \
                const before = c.length; \
                document.body.appendChild(document.createElement('img')); \
                return `${before},${c.length}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1,2");
}

#[test]
fn document_scripts_is_also_real_and_live() {
    let (mut d, body) = dom_with_body();
    let script1 = d.create_element("script");
    d.append_child(body, script1);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const c = document.scripts; \
                const before = c.length; \
                document.body.appendChild(document.createElement('script')); \
                return `${before},${c.length}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1,2");
}

#[test]
fn document_links_is_real_live_and_filters_by_href() {
    let (mut d, body) = dom_with_body();
    let a1 = d.create_element("a");
    d.set_attribute(a1, "href", "https://example.com");
    d.set_attribute(a1, "id", "l1");
    d.append_child(body, a1);
    let a_no_href = d.create_element("a");
    d.append_child(body, a_no_href);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const c = document.links; \
                const before = c.length; \
                const area = document.createElement('area'); \
                area.setAttribute('href', '/x'); \
                area.id = 'l2'; \
                document.body.appendChild(area); \
                return `${before},${c.length},${c[0].id},${c[1].id}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result, "1,2,l1,l2",
        "document.links should filter to href-bearing a/area, live, unioning both tags"
    );
}

#[test]
fn array_from_a_live_collection_still_works_via_the_array_like_path() {
    let (mut d, body) = dom_with_body();
    let div1 = d.create_element("div");
    d.set_attribute(div1, "id", "a");
    d.append_child(body, div1);
    let div2 = d.create_element("div");
    d.set_attribute(div2, "id", "b");
    d.append_child(body, div2);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const c = document.getElementsByTagName('div'); \
                return Array.from(c).map(e => e.id).join(','); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result, "a,b",
        "Array.from should still work via the real length+indexed-get array-like path, no ownKeys/iterator trap needed"
    );
}
