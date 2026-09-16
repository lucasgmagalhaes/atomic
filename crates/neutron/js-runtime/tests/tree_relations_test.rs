use js_runtime::{Context, Runtime};

#[test]
fn contains_and_is_connected_reflect_real_tree_attachment() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("main");
    let child = d.create_element("span");
    d.set_attribute(parent, "id", "parent");
    d.set_attribute(child, "id", "child");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const parent = document.getElementById('parent'); \
                const child = document.getElementById('child'); \
                const detached = document.createElement('div'); \
                return `${parent.contains(child)},${child.contains(parent)},${parent.contains(parent)},${parent.contains(detached)},${child.isConnected},${detached.isConnected}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false,true,false,true,false");
}

#[test]
fn clone_node_shallow_and_deep_produce_independent_copies() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("main");
    d.set_attribute(parent, "id", "parent");
    d.append_child(root, parent);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const original = document.createElement('div'); \
                original.setAttribute('class', 'widget'); \
                original.appendChild(document.createTextNode('hi')); \
                const shallow = original.cloneNode(false); \
                const deep = original.cloneNode(true); \
                shallow.setAttribute('class', 'changed'); \
                return `${shallow === original},${shallow.getAttribute('class')},${original.getAttribute('class')},${shallow.childNodes.length},${deep.textContent}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false,changed,widget,0,hi");
}

#[test]
fn replace_child_swaps_position_and_rejects_a_non_member_old_child() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("main");
    let old = d.create_element("span");
    d.set_attribute(parent, "id", "parent");
    d.set_attribute(old, "id", "old");
    d.append_child(root, parent);
    d.append_child(parent, old);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const parent = document.getElementById('parent'); \
                const old = document.getElementById('old'); \
                const replacement = document.createElement('em'); \
                replacement.id = 'new'; \
                const returned = parent.replaceChild(replacement, old); \
                let rejected = false; \
                try { parent.replaceChild(document.createElement('i'), document.createElement('b')); } catch (_) { rejected = true; } \
                return `${returned === old},${parent.firstChild === replacement},${document.getElementById('old')},${rejected}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,null,true");
}

#[test]
fn owner_document_is_the_same_shared_document_object() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let el = d.create_element("div");
    d.set_attribute(el, "id", "el");
    d.append_child(root, el);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "document.getElementById('el').ownerDocument === document",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn tag_name_local_name_and_namespace_uri_reflect_element_identity() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let el = d.create_element("DiV");
    d.set_attribute(el, "id", "el");
    d.append_child(root, el);
    let text = d.create_text("hi");
    d.append_child(el, text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('el'); \
                const text = el.firstChild; \
                return `${el.tagName},${el.localName},${el.namespaceURI},${text.tagName},${text.namespaceURI}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result,
        "DIV,DiV,http://www.w3.org/1999/xhtml,undefined,null"
    );
}

#[test]
fn document_create_comment_produces_a_real_comment_node() {
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
                const comment = document.createComment('note'); \
                document.getElementById('parent').appendChild(comment); \
                return `${comment.nodeType},${comment.nodeName},${document.getElementById('parent').outerHTML}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "8,#comment,<div id=\"parent\"><!--note--></div>");
}

#[test]
fn document_head_title_and_ready_state_are_real() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let html = d.create_element("html");
    let head = d.create_element("head");
    let title = d.create_element("title");
    let title_text = d.create_text("Original");
    d.append_child(root, html);
    d.append_child(html, head);
    d.append_child(head, title);
    d.append_child(title, title_text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const before = document.title; \
                document.title = 'Updated'; \
                return `${document.head === document.querySelector('head')},${before},${document.title},${document.readyState}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,Original,Updated,complete");
}

#[test]
fn document_title_setter_creates_a_title_element_when_none_exists() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let html = d.create_element("html");
    let head = d.create_element("head");
    d.append_child(root, html);
    d.append_child(html, head);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                document.title = 'Created'; \
                return `${document.title},${document.head.querySelector('title').textContent}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "Created,Created");
}

#[test]
fn get_elements_by_tag_name_and_class_name_are_real() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let container = d.create_element("main");
    d.set_attribute(container, "id", "container");
    let a = d.create_element("span");
    d.set_attribute(a, "class", "item highlight");
    let b = d.create_element("span");
    d.set_attribute(b, "class", "item");
    let c = d.create_element("p");
    d.set_attribute(c, "class", "item highlight");
    d.append_child(root, container);
    d.append_child(container, a);
    d.append_child(container, b);
    d.append_child(container, c);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const container = document.getElementById('container'); \
                const spans = container.getElementsByTagName('span'); \
                const allFromDoc = document.getElementsByTagName('*'); \
                const highlighted = document.getElementsByClassName('item highlight'); \
                const noneMatch = container.getElementsByClassName('missing'); \
                return `${spans.length},${allFromDoc.length >= 4},${highlighted.length},${noneMatch.length}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,true,2,0");
}
