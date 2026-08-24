use js_runtime::{Context, Runtime};

#[test]
fn restrictive_policy_blocks_clipboard_before_accessing_the_os_clipboard() {
    let runtime = Runtime::new();
    let mut context = Context::with_dom(&runtime, dom::Dom::new());
    context.set_permissions_policy("clipboard-read=(), clipboard-write=()");
    context
        .eval(
            "globalThis.write_error = null; navigator.clipboard.writeText('never reaches the OS').catch((error) => { write_error = error.message; });",
            "<test>",
        )
        .unwrap();
    context.run_pending_timers();

    assert_eq!(context.eval("write_error", "<test>").unwrap(), "clipboard write is blocked by Permissions Policy");
}

#[test]
fn restrictive_policy_blocks_notifications_and_does_not_grant_permission() {
    let runtime = Runtime::new();
    let mut context = Context::with_dom(&runtime, dom::Dom::new());
    context.set_permissions_policy("notifications=()");
    context
        .eval(
            "globalThis.result = null; Notification.requestPermission().then((permission) => { result = permission; });",
            "<test>",
        )
        .unwrap();
    context.run_pending_timers();

    assert_eq!(context.eval("result", "<test>").unwrap(), "denied");
    assert_eq!(context.eval("Notification.permission", "<test>").unwrap(), "default");
    assert!(context.eval("new Notification('blocked')", "<test>").is_err());
}

#[test]
fn self_and_wildcard_allow_the_top_level_document_capabilities() {
    let runtime = Runtime::new();
    let mut context = Context::with_dom(&runtime, dom::Dom::new());
    context.set_permissions_policy("notifications=(self), clipboard-write=(*)");
    context
        .eval(
            "globalThis.result = null; Notification.requestPermission().then((permission) => { result = permission; });",
            "<test>",
        )
        .unwrap();
    context.run_pending_timers();

    assert_eq!(context.eval("result", "<test>").unwrap(), "granted");
    assert_eq!(context.eval("new Notification('allowed').title", "<test>").unwrap(), "allowed");
}
