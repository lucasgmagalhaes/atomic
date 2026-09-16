use js_runtime::{Context, Runtime};

#[test]
fn inner_width_and_height_start_at_zero() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval("`${window.innerWidth},${window.innerHeight}`", "<test>")
        .unwrap();
    assert_eq!(result, "0,0");
}

#[test]
fn host_set_viewport_size_is_readable_from_script() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_viewport_size(1024.0, 768.0);
    let result = ctx
        .eval("`${window.innerWidth},${window.innerHeight}`", "<test>")
        .unwrap();
    assert_eq!(result, "1024,768");
}

#[test]
fn viewport_size_is_readable_by_the_host() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_viewport_size(800.0, 600.0);
    assert_eq!(ctx.viewport_width(), 800.0);
    assert_eq!(ctx.viewport_height(), 600.0);
}
