use js_runtime::{Context, Runtime};

#[test]
fn permission_starts_as_default() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert_eq!(ctx.eval("Notification.permission", "<test>").unwrap(), "default");
}

#[test]
fn permission_becomes_granted_only_after_request_permission_resolves() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert_eq!(ctx.eval("Notification.permission", "<test>").unwrap(), "default");

    ctx.eval("globalThis.seen = null; Notification.requestPermission().then((p) => { seen = p; });", "<test>").unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while ctx.eval("seen === null", "<test>").unwrap() == "true" && std::time::Instant::now() < deadline {
        ctx.run_pending_timers();
    }

    assert_eq!(ctx.eval("seen", "<test>").unwrap(), "granted");
    assert_eq!(ctx.eval("Notification.permission", "<test>").unwrap(), "granted");
}

#[test]
fn constructor_reads_back_real_title_and_options() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            r#"
            const n = new Notification('Claim ready', { body: 'Your idle claim is ready', tag: 'idle-claim', icon: '/icon.png' });
            `${n.title}|${n.body}|${n.tag}|${n.icon}`
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "Claim ready|Your idle claim is ready|idle-claim|/icon.png");
}

#[test]
fn constructor_without_options_leaves_extras_empty() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("`${new Notification('just a title').body}`", "<test>").unwrap();
    assert_eq!(result, "");
}

#[test]
fn close_does_not_throw_and_is_idempotent() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("const n = new Notification('x'); n.close(); n.close(); true", "<test>").unwrap();
    assert_eq!(result, "true");
}

#[test]
fn permission_is_isolated_per_context() {
    let rt = Runtime::new();
    let ctx1 = Context::new(&rt);
    let ctx2 = Context::new(&rt);

    ctx1.eval("Notification.requestPermission()", "<test>").unwrap();
    ctx1.run_pending_timers();

    assert_eq!(ctx1.eval("Notification.permission", "<test>").unwrap(), "granted");
    assert_eq!(ctx2.eval("Notification.permission", "<test>").unwrap(), "default");
}
