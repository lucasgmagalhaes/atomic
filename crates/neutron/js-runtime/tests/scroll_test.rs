use js_runtime::{Context, Runtime};

#[test]
fn scroll_y_starts_at_zero() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "`${window.scrollY},${window.pageYOffset},${window.scrollX}`",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,0,0");
}

#[test]
fn scroll_to_updates_scroll_y_and_fires_a_real_scroll_event() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            r#"(() => {
                let seen = null;
                window.addEventListener("scroll", () => { seen = window.scrollY; });
                window.scrollTo(0, 150);
                return `${window.scrollY},${seen}`;
            })()"#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "150,150");
}

#[test]
fn scroll_to_accepts_the_options_object_form() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(r#"window.scrollTo({top: 42}); window.scrollY"#, "<test>")
        .unwrap();
    assert_eq!(result, "42");
}

#[test]
fn scroll_by_adds_to_the_current_offset() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            r#"window.scrollTo(0, 100); window.scrollBy(0, 25); window.scrollY"#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "125");
}

#[test]
fn host_set_scroll_y_is_readable_from_script() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_scroll_y(80.0);
    let result = ctx.eval("window.scrollY", "<test>").unwrap();
    assert_eq!(result, "80");
}

#[test]
fn script_set_scroll_y_is_readable_by_the_host() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    ctx.eval("window.scrollTo(0, 60)", "<test>").unwrap();
    assert_eq!(ctx.scroll_y(), 60.0);
}
