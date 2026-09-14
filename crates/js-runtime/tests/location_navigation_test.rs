//! `location.href = ...`/`assign`/`replace`/`reload` — real write-side
//! navigation via `HostState.pending_navigation`, the same real channel
//! a real `<a href>` click's default action already uses (see
//! `anchor_click_test.rs`). Closes `spec/matrix/navigation-security.md`
//! line 20.

use js_runtime::{Context, Runtime};

fn eval(ctx: &Context, code: &str) {
    ctx.eval(code, "<test>").unwrap();
}

#[test]
fn setting_location_href_requests_navigation() {
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, dom::Dom::new());
    ctx.set_url("https://example.com/start");
    eval(&ctx, "location.href = 'https://example.com/next';");
    assert_eq!(
        ctx.take_pending_navigation(),
        Some("https://example.com/next".to_string())
    );
    assert_eq!(ctx.take_pending_navigation(), None);
}

#[test]
fn location_assign_requests_navigation() {
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, dom::Dom::new());
    ctx.set_url("https://example.com/start");
    eval(&ctx, "location.assign('/relative');");
    assert_eq!(ctx.take_pending_navigation(), Some("/relative".to_string()));
}

#[test]
fn location_replace_requests_navigation() {
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, dom::Dom::new());
    ctx.set_url("https://example.com/start");
    eval(&ctx, "location.replace('https://example.com/other');");
    assert_eq!(
        ctx.take_pending_navigation(),
        Some("https://example.com/other".to_string())
    );
}

#[test]
fn location_reload_requests_navigation_to_the_current_url() {
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, dom::Dom::new());
    ctx.set_url("https://example.com/here");
    eval(&ctx, "location.reload();");
    assert_eq!(
        ctx.take_pending_navigation(),
        Some("https://example.com/here".to_string())
    );
}

#[test]
fn location_href_getter_still_reflects_the_real_current_url() {
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, dom::Dom::new());
    ctx.set_url("https://example.com/page");
    let result = ctx.eval("location.href", "<test>").unwrap();
    assert_eq!(result, "https://example.com/page");
}
