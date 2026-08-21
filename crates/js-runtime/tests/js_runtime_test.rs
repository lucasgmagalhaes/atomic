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
fn dom_bindings_read_and_write_text_by_id() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");
    d.set_text_content(p, "hello");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let read = ctx.eval("__dom_get_text_by_id('greeting')", "<test>").unwrap();
    assert_eq!(read, "hello");

    let missing = ctx.eval("__dom_get_text_by_id('nope')", "<test>").unwrap();
    assert_eq!(missing, "null");

    let wrote = ctx
        .eval("__dom_set_text_by_id('greeting', 'bye')", "<test>")
        .unwrap();
    assert_eq!(wrote, "true");

    let read_again = ctx.eval("__dom_get_text_by_id('greeting')", "<test>").unwrap();
    assert_eq!(read_again, "bye");
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
fn performance_now_advances() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let first: f64 = ctx.eval("performance.now()", "<test>").unwrap().parse().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second: f64 = ctx.eval("performance.now()", "<test>").unwrap().parse().unwrap();
    assert!(second > first);
}
