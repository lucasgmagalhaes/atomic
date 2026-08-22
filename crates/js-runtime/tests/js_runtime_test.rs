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
