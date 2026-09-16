use js_runtime::{Context, Runtime};

fn eval(ctx: &Context, code: &str) -> String {
    ctx.eval(code, "<test>").unwrap()
}

#[test]
fn clicking_an_anchor_requests_navigation() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let anchor = d.create_element("a");
    d.set_attribute(anchor, "id", "link");
    d.set_attribute(anchor, "href", "https://example.com/next");
    d.append_child(root, anchor);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    eval(
        &ctx,
        "document.getElementById('link').dispatchEvent('click');",
    );
    assert_eq!(
        ctx.take_pending_navigation(),
        Some("https://example.com/next".to_string())
    );
    // Draining once must clear it - a second read sees nothing pending.
    assert_eq!(ctx.take_pending_navigation(), None);
}

#[test]
fn preventing_default_on_an_anchor_click_stops_the_navigation_request() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let anchor = d.create_element("a");
    d.set_attribute(anchor, "id", "link");
    d.set_attribute(anchor, "href", "https://example.com/next");
    d.append_child(root, anchor);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    eval(
        &ctx,
        r#"document.getElementById('link').addEventListener('click', (e) => e.preventDefault());
           document.getElementById('link').dispatchEvent('click');"#,
    );
    assert_eq!(ctx.take_pending_navigation(), None);
}

#[test]
fn clicking_an_anchor_with_no_href_requests_nothing() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let anchor = d.create_element("a");
    d.set_attribute(anchor, "id", "link");
    d.append_child(root, anchor);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    eval(
        &ctx,
        "document.getElementById('link').dispatchEvent('click');",
    );
    assert_eq!(ctx.take_pending_navigation(), None);
}
