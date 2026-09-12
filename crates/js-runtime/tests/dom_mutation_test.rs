use js_runtime::{Context, Runtime};

#[test]
fn get_element_by_id_returns_a_node_with_text_content() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");
    d.set_text_content(p, "hello");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let read = ctx
        .eval("document.getElementById('greeting').textContent", "<test>")
        .unwrap();
    assert_eq!(read, "hello");

    let missing = ctx
        .eval("document.getElementById('nope')", "<test>")
        .unwrap();
    assert_eq!(missing, "null");

    let wrote = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('greeting'); \
                el.textContent = 'bye'; \
                return el.textContent; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(wrote, "bye");
}

#[test]
fn scripts_can_create_append_query_and_remove_dom_nodes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("main");
    d.set_attribute(parent, "id", "parent");
    d.append_child(root, parent);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const parent = document.getElementById('parent'); const child = document.createElement('article'); child.textContent = 'created'; const appended = parent.appendChild(child); const found = parent.querySelector('article'); child.remove(); return `${appended === child},${found === child},${document.querySelector('article')}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,true,null");
}

#[test]
fn dom_mutation_rejects_invalid_tags_and_cycles() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    let child = d.create_element("span");
    d.set_attribute(parent, "id", "parent");
    d.set_attribute(child, "id", "child");
    d.append_child(root, parent);
    d.append_child(parent, child);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const parent = document.getElementById('parent'); const child = document.getElementById('child'); let invalid = false; let cycle = false; try { document.createElement('<script>'); } catch (_) { invalid = true; } try { child.appendChild(parent); } catch (_) { cycle = true; } return `${invalid},${cycle},${parent.querySelector('span') === child}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,true,true");
}

#[test]
fn attributes_on_created_nodes_are_bounded_and_visible_to_selectors() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("main");
    d.set_attribute(parent, "id", "parent");
    d.append_child(root, parent);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const child = document.createElement('button'); child.setAttribute('id', 'dynamic'); child.setAttribute('class', 'action primary'); document.getElementById('parent').appendChild(child); let invalid = false; try { child.setAttribute('<bad>', 'x'); } catch (_) { invalid = true; } child.removeAttribute('class'); return `${child.getAttribute('id')},${document.querySelector('.primary')},${child.getAttribute('missing')},${invalid}`; })()", "<test>").unwrap();
    assert_eq!(result, "dynamic,null,null,true");
}

#[test]
fn scripts_can_insert_text_before_a_sibling_and_remove_children() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("main");
    let tail = d.create_element("span");
    d.set_attribute(parent, "id", "parent");
    d.set_attribute(tail, "id", "tail");
    d.append_child(root, parent);
    d.append_child(parent, tail);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const parent = document.getElementById('parent'); const tail = document.getElementById('tail'); const text = document.createTextNode('before'); const inserted = parent.insertBefore(text, tail); const removed = parent.removeChild(tail); return `${inserted === text},${parent.textContent},${removed === tail},${document.getElementById('tail')}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,before,true,null");
}

#[test]
fn node_navigation_exposes_stable_tree_relationships() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("main");
    let first = d.create_element("span");
    let second = d.create_element("button");
    d.set_attribute(parent, "id", "parent");
    d.set_attribute(first, "id", "first");
    d.set_attribute(second, "id", "second");
    d.append_child(root, parent);
    d.append_child(parent, first);
    d.append_child(parent, second);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const parent = document.getElementById('parent'); const first = document.getElementById('first'); const second = document.getElementById('second'); return `${parent.childNodes.length},${parent.firstChild === first},${parent.lastChild === second},${first.nextSibling === second},${second.previousSibling === first},${first.parentNode === parent},${first.nodeType},${first.nodeName}`; })()", "<test>").unwrap();
    assert_eq!(result, "2,true,true,true,true,true,1,SPAN");
}

#[test]
fn id_and_class_name_properties_update_selector_state() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("main");
    d.set_attribute(parent, "id", "parent");
    d.append_child(root, parent);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const child = document.createElement('button'); child.id = 'dynamic'; child.className = 'action primary'; document.getElementById('parent').appendChild(child); return `${child.id},${child.className},${document.querySelector('#dynamic') === child},${document.querySelector('.primary') === child}`; })()", "<test>").unwrap();
    assert_eq!(result, "dynamic,action primary,true,true");
}

#[test]
fn matches_and_closest_reuse_the_supported_selector_grammar() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let section = d.create_element("section");
    let button = d.create_element("button");
    d.set_attribute(section, "class", "panel");
    d.set_attribute(section, "id", "section");
    d.set_attribute(button, "class", "action");
    d.set_attribute(button, "id", "button");
    d.append_child(root, section);
    d.append_child(section, button);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const section = document.getElementById('section'); const button = document.getElementById('button'); return `${button.matches('section > button.action')},${button.matches('.panel')},${button.closest('.panel') === section},${button.closest('article')}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,false,true,null");
}
