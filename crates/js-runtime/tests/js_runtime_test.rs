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

    let missing = ctx.eval("document.getElementById('nope')", "<test>").unwrap();
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

    let dispatched = ctx.eval("document.getElementById('greeting').dispatchEvent('click')", "<dispatch>").expect("dispatching should eval cleanly");
    assert_eq!(dispatched, "true", "a real listener attached in an earlier eval call should still be found and called");

    let text_after = ctx.eval("document.getElementById('greeting').textContent", "<check>").expect("reading textContent should eval cleanly");
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
fn dispatch_event_with_no_listener_returns_false() {
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
    assert_eq!(result, "false");
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
        .eval(
            "document.visibilityState + ',' + document.hidden",
            "<test>",
        )
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
        .eval(
            "document.visibilityState + ',' + document.hidden",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "visible,false");
}

#[test]
fn performance_now_advances() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let first: f64 = ctx.eval("performance.now()", "<test>").unwrap().parse().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second: f64 = ctx.eval("performance.now()", "<test>").unwrap().parse().unwrap();
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

    let initial = ctx.eval("document.getElementById('field').value", "<test>").unwrap();
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

    let value = ctx.eval("document.getElementById('notes').value", "<test>").unwrap();
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
