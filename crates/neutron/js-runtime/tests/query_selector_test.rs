use js_runtime::{Context, Runtime};

#[test]
fn query_selector_uses_the_existing_css_selector_subset_in_document_order() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let section = d.create_element("section");
    d.append_child(root, section);
    let first = d.create_element("button");
    d.set_attribute(first, "class", "claim");
    d.set_attribute(first, "id", "first");
    d.append_child(section, first);
    let second = d.create_element("button");
    d.set_attribute(second, "class", "claim");
    d.set_attribute(second, "id", "second");
    d.append_child(section, second);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { const all = document.querySelectorAll('section > button.claim'); return all.length + ',' + all[0].textContent + ',' + (document.querySelector('#second') === all[1]) + ',' + (all[0] === document.getElementById('first')); })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,,true,true");
}

#[test]
fn element_query_selector_excludes_the_receiver_and_invalid_selectors_throw() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    d.set_attribute(container, "id", "container");
    d.append_child(root, container);
    let child = d.create_element("div");
    d.set_attribute(child, "class", "child");
    d.append_child(container, child);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { const container = document.getElementById('container'); const found = container.querySelectorAll('div'); let invalid = false; try { document.querySelector(':not(div)'); } catch (_) { invalid = true; } return found.length + ',' + (found[0] !== container) + ',' + invalid; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1,true,true");
}

#[test]
fn get_element_by_id_returns_distinct_node_objects_for_the_same_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let same_underlying_node = ctx
        .eval(
            "(() => { \
                const a = document.getElementById('greeting'); \
                const b = document.getElementById('greeting'); \
                a.textContent = 'via a'; \
                return b.textContent; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(same_underlying_node, "via a");
}
