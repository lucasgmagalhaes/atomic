use js_runtime::{Context, Runtime};

#[test]
fn evals_arithmetic() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("1 + 2", "<test>").unwrap();
    assert_eq!(result, "3");
}

#[test]
fn evals_string_concat() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("'a' + 'b'", "<test>").unwrap();
    assert_eq!(result, "ab");
}

#[test]
fn reports_exception_as_err() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("throw new Error('boom')", "<test>");
    assert!(result.is_err());
}

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

#[test]
fn constructed_events_are_dispatched_by_identity() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let button = d.create_element("button");
    d.set_attribute(button, "id", "button");
    d.append_child(root, button);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const button = document.getElementById('button'); const event = new Event('save'); let seen = false; button.addEventListener('save', received => { seen = received === event && received.type === 'save' && received.target === button && received.currentTarget === button && !received.bubbles && !received.cancelable; }); return `${button.dispatchEvent(event)},${seen}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,true");
}

#[test]
fn constructed_event_options_control_bubbling_and_cancellation() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("section");
    let button = d.create_element("button");
    d.set_attribute(parent, "id", "parent");
    d.set_attribute(button, "id", "button");
    d.append_child(root, parent);
    d.append_child(parent, button);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const parent = document.getElementById('parent'); const button = document.getElementById('button'); let bubbled = false; parent.addEventListener('save', event => { bubbled = event.bubbles; event.preventDefault(); }); const event = new Event('save', { bubbles: true, cancelable: true, ignored: true }); return `${button.dispatchEvent(event)},${bubbled},${event.defaultPrevented}`; })()", "<test>").unwrap();
    assert_eq!(result, "false,true,true");
}

#[test]
fn document_body_is_stable_and_accepts_dynamic_children() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let html = d.create_element("html");
    let body = d.create_element("body");
    d.append_child(root, html);
    d.append_child(html, body);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const child = document.createElement('main'); child.id = 'app'; document.body.appendChild(child); return `${document.documentElement === document.querySelector('html')},${document.body === document.querySelector('body')},${document.body.querySelector('#app') === child}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,true,true");
}

#[test]
fn class_list_is_stable_and_updates_the_class_attribute() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let element = d.create_element("div");
    d.set_attribute(element, "class", "first first");
    d.append_child(root, element);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const el = document.querySelector('div'); const list = el.classList; list.add('second'); list.remove('first'); return `${list === el.classList},${list.contains('second')},${el.className}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,true,second");
}

#[test]
fn class_list_supports_toggle_replace_value_length_and_iteration() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let element = d.create_element("div");
    d.set_attribute(element, "class", "a b");
    d.append_child(root, element);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const el = document.querySelector('div'); \
                const list = el.classList; \
                const toggledOn = list.toggle('c'); \
                const toggledOff = list.toggle('a'); \
                const forced = list.toggle('d', true); \
                const replaced = list.replace('b', 'e'); \
                const lengthAfterMutations = list.length; \
                list.value = 'x y z'; \
                const joined = [...list].join(','); \
                return `${toggledOn},${toggledOff},${forced},${replaced},${lengthAfterMutations},${list.length},${joined},${el.className}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false,true,true,3,3,x,y,z,x y z");
}

#[test]
fn dataset_reflects_live_data_attributes_by_camel_case_key() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let element = d.create_element("div");
    d.set_attribute(element, "data-user-id", "42");
    d.set_attribute(element, "data-role", "admin");
    d.append_child(root, element);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const el = document.querySelector('div'); \
                const ds = el.dataset; \
                const before = `${ds.userId},${ds.role},${ds === el.dataset}`; \
                el.removeAttribute('data-role'); \
                el.setAttribute('data-user-id', '43'); \
                return `${before},${el.dataset.userId},${el.dataset.role}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "42,admin,true,43,undefined");
}

#[test]
fn attributes_collection_is_stable_iterable_and_supports_get_named_item() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let element = d.create_element("div");
    d.set_attribute(element, "id", "panel");
    d.set_attribute(element, "data-role", "admin");
    d.append_child(root, element);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const el = document.querySelector('div'); \
                const attrs = el.attributes; \
                const stable = attrs === el.attributes; \
                const pairs = [...attrs].map(a => `${a.name}=${a.value}`).join(','); \
                const named = attrs.getNamedItem('id').value; \
                const missing = attrs.getNamedItem('nope'); \
                el.removeAttribute('data-role'); \
                return `${stable},${pairs},${named},${missing},${el.attributes.length}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,data-role=admin,id=panel,panel,null,1");
}

#[test]
fn form_and_anchor_specific_properties_reflect_boolean_and_href_attributes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    let anchor = d.create_element("a");
    d.set_attribute(anchor, "href", "https://example.com/");
    d.append_child(root, input);
    d.append_child(root, anchor);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const input = document.querySelectorAll('input')[0]; \
                const anchor = document.querySelectorAll('a')[0]; \
                const before = `${input.checked},${input.disabled}`; \
                input.checked = true; \
                input.disabled = true; \
                const after = `${input.checked},${input.hasAttribute('checked')},${input.disabled}`; \
                input.checked = false; \
                const cleared = `${input.checked},${input.hasAttribute('checked')}`; \
                return `${before},${after},${cleared},${anchor.href}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result,
        "false,false,true,true,true,false,false,https://example.com/"
    );
}

#[test]
fn attribute_presence_and_names_follow_live_dom_attributes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let element = d.create_element("div");
    d.set_attribute(element, "data-state", "ready");
    d.set_attribute(element, "id", "panel");
    d.append_child(root, element);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const el = document.querySelector('div'); const before = `${el.hasAttribute('id')},${el.getAttributeNames().join(',')}`; el.removeAttribute('id'); return `${before},${el.hasAttribute('id')},${el.getAttributeNames().join(',')}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,data-state,id,false,data-state");
}

#[test]
fn name_and_type_properties_reflect_live_attributes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const input = document.querySelector('input'); input.name = 'email'; input.type = 'email'; return `${input.getAttribute('name')},${input.getAttribute('type')},${input.name},${input.type}`; })()", "<test>").unwrap();
    assert_eq!(result, "email,email,email,email");
}

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

#[test]
fn a_listener_attached_in_one_eval_call_survives_to_dispatch_in_a_later_separate_eval_call() {
    // Real bug this guards against: `getElementById` used to build a
    // brand-new JS object every call, so a listener attached to the
    // object from *one* `eval()` was invisible to a `dispatchEvent` from
    // a *different*, later `eval()` call on the same node id (the common
    // real-world shape: attach a listener at page load, dispatch later
    // from a real user interaction or a separate command) - `__listeners`
    // lived on the now-discarded first wrapper object, not the fresh one
    // the second `getElementById` call built. `Node` object identity is
    // now cached per real `dom::NodeId` (see `dom_bindings::get_or_create_node_object`),
    // so this must pass with the attach and the dispatch in two entirely
    // separate top-level `eval()` calls, not one expression.
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");
    d.set_text_content(p, "before");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    ctx.eval(
        "document.getElementById('greeting').addEventListener('click', () => { document.getElementById('greeting').textContent = 'clicked'; });",
        "<attach>",
    )
    .expect("attaching the listener should eval cleanly");

    let dispatched = ctx
        .eval(
            "document.getElementById('greeting').dispatchEvent('click')",
            "<dispatch>",
        )
        .expect("dispatching should eval cleanly");
    assert_eq!(
        dispatched, "true",
        "a real listener attached in an earlier eval call should still be found and called"
    );

    let text_after = ctx
        .eval("document.getElementById('greeting').textContent", "<check>")
        .expect("reading textContent should eval cleanly");
    assert_eq!(text_after, "clicked", "the listener's real mutation should be visible through yet another fresh getElementById call");
}

#[test]
fn add_event_listener_and_dispatch_event_calls_the_listener() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");
    d.set_text_content(p, "before");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('greeting'); \
                el.addEventListener('click', () => { el.textContent = 'clicked'; }); \
                const dispatched = el.dispatchEvent('click'); \
                return dispatched + ',' + el.textContent; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,clicked");
}

#[test]
fn event_listeners_run_in_registration_order_and_can_be_removed_individually() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "target");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { const el = document.getElementById('target'); let log = ''; const first = () => { log += 'a'; }; const second = () => { log += 'b'; }; el.addEventListener('click', first); el.addEventListener('click', second); el.removeEventListener('click', first); el.dispatchEvent('click'); return log; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "b");
}

#[test]
fn dispatch_event_with_no_listener_is_not_canceled() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "document.getElementById('greeting').dispatchEvent('click')",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn events_expose_target_and_current_target_bubble_and_can_be_canceled() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("section");
    let child = d.create_element("button");
    d.append_child(root, parent);
    d.append_child(parent, child);
    d.set_attribute(parent, "id", "parent");
    d.set_attribute(child, "id", "child");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const parent = document.getElementById('parent'); const child = document.getElementById('child'); let log = ''; child.addEventListener('click', event => { log += `${event.type}:${event.target === child}:${event.currentTarget === child}:${event.bubbles}:${event.cancelable}`; }); parent.addEventListener('click', event => { log += `|parent:${event.target === child}:${event.currentTarget === parent}`; event.preventDefault(); }); return `${child.dispatchEvent('click')},${log}`; })()", "<test>").unwrap();
    assert_eq!(result, "false,click:true:true:true:true|parent:true:true");
}

#[test]
fn stop_propagation_prevents_later_ancestors() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let outer = d.create_element("main");
    let inner = d.create_element("section");
    let child = d.create_element("button");
    d.append_child(root, outer);
    d.append_child(outer, inner);
    d.append_child(inner, child);
    d.set_attribute(outer, "id", "outer");
    d.set_attribute(inner, "id", "inner");
    d.set_attribute(child, "id", "child");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const outer = document.getElementById('outer'); const inner = document.getElementById('inner'); const child = document.getElementById('child'); let log = ''; outer.addEventListener('click', () => { log += 'outer'; }); inner.addEventListener('click', event => { log += 'inner'; event.stopPropagation(); }); child.dispatchEvent('click'); return log; })()", "<test>").unwrap();
    assert_eq!(result, "inner");
}

#[test]
fn nested_dispatch_is_bounded_without_leaking_depth_between_events() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let button = d.create_element("button");
    d.append_child(root, button);
    d.set_attribute(button, "id", "button");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const button = document.getElementById('button'); let calls = 0; button.addEventListener('loop', () => { calls++; button.dispatchEvent('loop'); }); let limited = false; try { button.dispatchEvent('loop'); } catch (_) { limited = true; } const first = calls; button.removeEventListener('loop'); button.addEventListener('done', () => { calls++; }); const second = button.dispatchEvent('done'); return `${first},${limited},${second},${calls}`; })()", "<test>").unwrap();
    let parts: Vec<&str> = result.split(',').collect();
    let first: u32 = parts[0].parse().unwrap();
    let limited = parts[1] == "true";
    let second = parts[2] == "true";
    let calls: u32 = parts[3].parse().unwrap();
    assert!(
        first > 1,
        "recursion should have run at least twice before being bounded"
    );
    assert!(limited, "nested dispatch should have been bounded");
    assert!(second, "subsequent dispatch should succeed");
    assert_eq!(calls, first + 1);
}

#[test]
fn event_listener_limit_throws_without_registering_an_extra_callback() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let button = d.create_element("button");
    d.append_child(root, button);
    d.set_attribute(button, "id", "button");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const button = document.getElementById('button'); let calls = 0; const listener = () => { calls++; }; for (let i = 0; i < 64; i++) button.addEventListener('click', listener); let limited = false; try { button.addEventListener('click', listener); } catch (_) { limited = true; } button.dispatchEvent('click'); return `${limited},${calls}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,64");
}

#[test]
fn event_propagation_limit_throws_for_a_deep_dom_tree() {
    let mut d = dom::Dom::new();
    let mut parent = d.root();
    for _ in 0..129 {
        let child = d.create_element("div");
        d.append_child(parent, child);
        parent = child;
    }
    d.set_attribute(parent, "id", "target");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { try { document.getElementById('target').dispatchEvent('click'); return 'not-limited'; } catch (_) { return 'limited'; } })()", "<test>").unwrap();
    assert_eq!(result, "limited");
}

#[test]
fn listener_exceptions_are_rethrown_after_later_listeners_run() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let button = d.create_element("button");
    d.append_child(root, button);
    d.set_attribute(button, "id", "button");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const button = document.getElementById('button'); let log = ''; button.addEventListener('click', () => { throw new Error('expected'); }); button.addEventListener('click', () => { log += 'later'; }); try { button.dispatchEvent('click'); } catch (_) { log += ':caught'; } return log; })()", "<test>").unwrap();
    assert_eq!(result, "later:caught");
}

#[test]
fn remove_event_listener_stops_future_dispatch() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('greeting'); \
                let calls = 0; \
                const handler = () => { calls++; }; \
                el.addEventListener('click', handler); \
                el.removeEventListener('click'); \
                el.dispatchEvent('click'); \
                return calls; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0");
}

#[test]
fn performance_now_is_a_nonnegative_number() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("typeof performance.now()", "<test>").unwrap();
    assert_eq!(result, "number");
}

#[test]
fn crypto_get_random_values_fills_and_returns_the_array() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    // Sum the bytes so we can tell it's not all-zero, and check the return
    // value is the same array (length preserved, chainable per spec).
    let result = ctx
        .eval(
            "(() => { \
                const a = new Uint8Array(16); \
                const b = crypto.getRandomValues(a); \
                return (b === a) + ',' + b.length; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,16");
}

#[test]
fn page_visibility_and_dom_bindings_coexist_on_shared_document() {
    // Regression: dom_bindings and page_visibility both used to create
    // their own `document` object, so with_dom() silently dropped
    // whichever one registered first (visibilityState/hidden, since
    // Context::new runs before with_dom's dom_bindings::register).
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");
    d.set_text_content(p, "hello");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let visibility = ctx
        .eval("document.visibilityState + ',' + document.hidden", "<test>")
        .unwrap();
    assert_eq!(visibility, "visible,false");

    let text = ctx
        .eval("document.getElementById('greeting').textContent", "<test>")
        .unwrap();
    assert_eq!(text, "hello");
}

#[test]
fn page_visibility_reports_visible() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("document.visibilityState + ',' + document.hidden", "<test>")
        .unwrap();
    assert_eq!(result, "visible,false");
}

#[test]
fn performance_now_advances() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let first: f64 = ctx
        .eval("performance.now()", "<test>")
        .unwrap()
        .parse()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second: f64 = ctx
        .eval("performance.now()", "<test>")
        .unwrap()
        .parse()
        .unwrap();
    assert!(second > first);
}

#[test]
fn node_value_is_real_and_independent_of_text_content() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let initial = ctx
        .eval("document.getElementById('field').value", "<test>")
        .unwrap();
    assert_eq!(initial, "");

    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('field'); \
                el.value = 'typed'; \
                return el.value + ',' + el.textContent; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "typed,");
}

#[test]
fn textarea_value_falls_back_to_text_content_until_set() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let textarea = d.create_element("textarea");
    d.append_child(root, textarea);
    d.set_attribute(textarea, "id", "notes");
    d.set_text_content(textarea, "seeded");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let value = ctx
        .eval("document.getElementById('notes').value", "<test>")
        .unwrap();
    assert_eq!(value, "seeded");
}

#[test]
fn focus_blur_and_active_element_are_real() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let before = ctx.eval("document.activeElement", "<test>").unwrap();
    assert_eq!(before, "null");

    let after_focus = ctx
        .eval(
            "(() => { document.getElementById('field').focus(); return document.activeElement === document.getElementById('field'); })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(after_focus, "true");

    let after_blur = ctx
        .eval(
            "(() => { document.getElementById('field').blur(); return document.activeElement; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(after_blur, "null");
}

#[test]
fn focus_dispatches_a_real_focus_event() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let fired = ctx
        .eval(
            "(() => { \
                let seen = false; \
                const el = document.getElementById('field'); \
                el.addEventListener('focus', () => { seen = true; }); \
                el.focus(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(fired, "true");
}

#[test]
fn blur_dispatches_a_real_blur_event() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let fired = ctx
        .eval(
            "(() => { \
                let seen = false; \
                const el = document.getElementById('field'); \
                el.addEventListener('blur', () => { seen = true; }); \
                el.focus(); \
                el.blur(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(fired, "true");
}

#[test]
fn blur_is_a_no_op_and_does_not_dispatch_when_the_node_is_not_focused() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let fired = ctx
        .eval(
            "(() => { \
                let seen = false; \
                const el = document.getElementById('field'); \
                el.addEventListener('blur', () => { seen = true; }); \
                el.blur(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(fired, "false");
}

#[test]
fn setting_value_dispatches_a_real_input_event() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let seen_value = ctx
        .eval(
            "(() => { \
                let seenValue = null; \
                const el = document.getElementById('field'); \
                el.addEventListener('input', () => { seenValue = el.value; }); \
                el.value = 'typed'; \
                return seenValue; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(seen_value, "typed");
}

#[test]
fn blur_dispatches_a_real_change_event_only_when_value_moved_since_focus() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let changed_after_edit = ctx
        .eval(
            "(() => { \
                let seen = false; \
                const el = document.getElementById('field'); \
                el.addEventListener('change', () => { seen = true; }); \
                el.focus(); \
                el.value = 'typed'; \
                el.blur(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(changed_after_edit, "true");

    let changed_without_edit = ctx
        .eval(
            "(() => { \
                let seen = false; \
                const el = document.getElementById('field'); \
                el.addEventListener('change', () => { seen = true; }); \
                el.focus(); \
                el.blur(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(changed_without_edit, "false");
}

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

#[test]
fn prepend_and_append_insert_mixed_node_and_string_args_in_order() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("ul");
    d.set_attribute(parent, "id", "parent");
    let middle = d.create_element("li");
    d.set_attribute(middle, "id", "middle");
    d.append_child(root, parent);
    d.append_child(parent, middle);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const parent = document.getElementById('parent'); \
                const first = document.createElement('li'); \
                first.id = 'first'; \
                parent.prepend(first, 'lead-text'); \
                const last = document.createElement('li'); \
                last.id = 'last'; \
                parent.append('tail-text', last); \
                return Array.from(parent.childNodes).map(n => n.id || n.textContent).join(','); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "first,lead-text,middle,tail-text,last");
}

#[test]
fn before_and_after_insert_relative_to_the_reference_node() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    let anchor = d.create_element("span");
    d.set_attribute(anchor, "id", "anchor");
    d.append_child(root, parent);
    d.append_child(parent, anchor);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const parent = document.getElementById('parent'); \
                const anchor = document.getElementById('anchor'); \
                const before1 = document.createElement('b'); \
                before1.id = 'before1'; \
                const before2 = document.createElement('b'); \
                before2.id = 'before2'; \
                anchor.before(before1, before2); \
                const after1 = document.createElement('i'); \
                after1.id = 'after1'; \
                const after2 = document.createElement('i'); \
                after2.id = 'after2'; \
                anchor.after(after1, after2); \
                const detached = document.createElement('u'); \
                let detachedNoOp = true; \
                detached.before(document.createElement('span')); \
                detachedNoOp = detached.parentNode === null; \
                return `${Array.from(parent.childNodes).map(n => n.id).join(',')},${detachedNoOp}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "before1,before2,anchor,after1,after2,true");
}

#[test]
fn replace_with_swaps_position_and_frees_the_old_node() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    let first = d.create_element("span");
    d.set_attribute(first, "id", "first");
    let old = d.create_element("span");
    d.set_attribute(old, "id", "old");
    let last = d.create_element("span");
    d.set_attribute(last, "id", "last");
    d.append_child(root, parent);
    d.append_child(parent, first);
    d.append_child(parent, old);
    d.append_child(parent, last);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const old = document.getElementById('old'); \
                const a = document.createElement('em'); \
                a.id = 'a'; \
                const b = document.createElement('em'); \
                b.id = 'b'; \
                old.replaceWith(a, 'text-between', b); \
                const parent = document.getElementById('parent'); \
                return `${Array.from(parent.childNodes).map(n => n.id || n.textContent).join(',')},${document.getElementById('old')}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "first,a,text-between,b,last,null");
}

#[test]
fn child_node_insertion_methods_reject_a_string_arg_treated_as_a_node_cycle() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    let child = d.create_element("span");
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
                let cycleRejected = false; \
                try { child.append(parent); } catch (_) { cycleRejected = true; } \
                let badArgRejected = false; \
                try { parent.append(42); } catch (_) { badArgRejected = true; } \
                return `${cycleRejected},${badArgRejected},${parent.querySelector('#child') === child}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true");
}

#[test]
fn insert_adjacent_html_inserts_at_all_four_real_positions() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let outer = d.create_element("section");
    d.set_attribute(outer, "id", "outer");
    let target = d.create_element("div");
    d.set_attribute(target, "id", "target");
    let existing_child = d.create_element("span");
    d.set_attribute(existing_child, "id", "existing");
    d.append_child(root, outer);
    d.append_child(outer, target);
    d.append_child(target, existing_child);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const target = document.getElementById('target'); \
                target.insertAdjacentHTML('beforebegin', '<p id=\"before\">before</p>'); \
                target.insertAdjacentHTML('afterend', '<p id=\"after\">after</p>'); \
                target.insertAdjacentHTML('afterbegin', '<b id=\"first\">first</b>'); \
                target.insertAdjacentHTML('beforeend', '<b id=\"last\">last</b>'); \
                const outer = document.getElementById('outer'); \
                return `${Array.from(outer.childNodes).map(n => n.id).join(',')},${Array.from(target.childNodes).map(n => n.id).join(',')}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "before,target,after,first,existing,last");
}

#[test]
fn insert_adjacent_html_rejects_bad_position_and_no_parent_cases() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let target = d.create_element("div");
    d.set_attribute(target, "id", "target");
    d.append_child(root, target);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const target = document.getElementById('target'); \
                let badPositionRejected = false; \
                try { target.insertAdjacentHTML('nowhere', '<b></b>'); } catch (_) { badPositionRejected = true; } \
                const detached = document.createElement('div'); \
                let noParentRejected = false; \
                try { detached.insertAdjacentHTML('beforebegin', '<b></b>'); } catch (_) { noParentRejected = true; } \
                detached.insertAdjacentHTML('afterbegin', '<b id=\"ok\">ok</b>'); \
                return `${badPositionRejected},${noParentRejected},${detached.firstChild.id}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,ok");
}

#[test]
fn compare_document_position_returns_following_for_later_sibling() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let a = d.create_element("div");
    let b = d.create_element("span");
    d.set_attribute(a, "id", "a");
    d.set_attribute(b, "id", "b");
    d.append_child(root, a);
    d.append_child(root, b);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const a = document.getElementById('a'); const b = document.getElementById('b'); return `${a.compareDocumentPosition(b) & 4},${b.compareDocumentPosition(a) & 2},${a.compareDocumentPosition(a)}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "4,2,0");
}

#[test]
fn compare_document_position_contains_and_contained_by() {
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
    let result = ctx
        .eval(
            "(() => { const p = document.getElementById('parent'); const c = document.getElementById('child'); return `${p.compareDocumentPosition(c) & 16},${c.compareDocumentPosition(p) & 8}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "16,8");
}

#[test]
fn compare_document_position_disconnected_nodes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let a = d.create_element("div");
    let b = d.create_element("span");
    d.set_attribute(a, "id", "a");
    d.set_attribute(b, "id", "b");
    d.append_child(root, a);
    // b is detached

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const a = document.getElementById('a'); const b = document.getElementById('b'); return `${a.compareDocumentPosition(b) & 1}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1");
}

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

#[test]
fn element_instanceof_node_element_html_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('div'); return `${el instanceof Node},${el instanceof Element},${el instanceof HTMLElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true");
}

#[test]
fn text_node_instanceof_node_only() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);
    let text = d.create_text("hello");
    d.append_child(div, text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const t = document.querySelector('div').firstChild; return `${t.nodeType},${t instanceof Node},${t instanceof Element},${t instanceof HTMLElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "3,true,false,false");
}

#[test]
fn input_element_instanceof_html_input_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('input'); return `${el instanceof Node},${el instanceof Element},${el instanceof HTMLElement},${el instanceof HTMLInputElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true,true");
}

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
#[test]
fn button_element_instanceof_html_button_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let btn = d.create_element("button");
    d.append_child(root, btn);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('button'); return `${el instanceof HTMLButtonElement},${el instanceof HTMLInputElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false");
}

#[test]
fn anchor_element_instanceof_html_anchor_element() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let a = d.create_element("a");
    d.append_child(root, a);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('a'); return `${el instanceof HTMLAnchorElement},${el instanceof HTMLInputElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false");
}

#[test]
fn generic_element_not_instanceof_input() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('div'); return `${el instanceof HTMLInputElement},${el instanceof HTMLButtonElement},${el instanceof HTMLAnchorElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false,false,false");
}

#[test]
fn node_constructor_exists_and_prototype_chain() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { return `${typeof Node},${typeof Element},${typeof HTMLElement},${typeof HTMLInputElement}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "function,function,function,function");
}

#[test]
fn cloned_element_preserves_class_hierarchy() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const orig = document.querySelector('input'); const clone = orig.cloneNode(true); return `${clone instanceof HTMLInputElement},${clone instanceof HTMLElement},${clone instanceof Element},${clone instanceof Node}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true,true");
}

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

#[test]
fn scrolltop_and_scrollleft_return_zero() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('div'); return `${el.scrollTop},${el.scrollLeft}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,0");
}

#[test]
fn scrolltop_setter_is_noop() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('div'); el.scrollTop = 100; el.scrollLeft = 50; return `${el.scrollTop},${el.scrollLeft}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,0");
}

#[test]
fn scroll_methods_are_callable_without_error() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('div'); el.scroll(); el.scrollTo(10, 20); el.scrollBy(5, 5); el.scrollIntoView(); return 'ok'; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "ok");
}

#[test]
fn scrolltop_is_inherited_by_html_elements() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('div'); return `${typeof el.scrollTop},${typeof el.scrollLeft},${typeof el.scroll},${typeof el.scrollTo},${typeof el.scrollBy},${typeof el.scrollIntoView}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "number,number,function,function,function,function");
}

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

#[test]
fn select_options_selected_index_and_value_work_together() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let select = d.create_element("select");
    d.append_child(body, select);
    for (i, label) in ["red", "green", "blue"].iter().enumerate() {
        let option = d.create_element("option");
        if i == 1 {
            d.set_attribute(option, "selected", "");
        }
        d.set_text_content(option, label);
        d.append_child(select, option);
    }

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const s = document.querySelector('select'); return `${s instanceof HTMLSelectElement},${s.options.length},${s.selectedIndex},${s.value}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,3,1,green");
}

#[test]
fn select_selected_index_setter_moves_selection() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let select = d.create_element("select");
    d.append_child(body, select);
    for (i, label) in ["a", "b", "c"].iter().enumerate() {
        let option = d.create_element("option");
        if i == 0 {
            d.set_attribute(option, "selected", "");
        }
        d.set_text_content(option, label);
        d.append_child(select, option);
    }

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const s = document.querySelector('select'); s.selectedIndex = 2; return `${s.selectedIndex},${s.value}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,c");
}

#[test]
fn select_value_setter_selects_matching_option() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let select = d.create_element("select");
    d.append_child(body, select);
    for value in ["x", "y", "z"] {
        let option = d.create_element("option");
        d.set_attribute(option, "value", value);
        d.set_text_content(option, &format!("label-{value}"));
        d.append_child(select, option);
    }

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const s = document.querySelector('select'); s.value = 'z'; return `${s.selectedIndex},${s.value}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,z");
}

#[test]
fn fresh_select_without_explicit_selection_reads_empty() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let select = d.create_element("select");
    d.append_child(body, select);
    let option = d.create_element("option");
    d.set_text_content(option, "only");
    d.append_child(select, option);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const s = document.querySelector('select'); return `${s.selectedIndex},${s.value === ''}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "-1,true");
}
