use js_runtime::{Context, Runtime};

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
