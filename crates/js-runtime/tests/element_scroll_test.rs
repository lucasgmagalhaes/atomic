use js_runtime::{Context, Runtime};

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
