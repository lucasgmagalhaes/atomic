use std::time::Duration;

use js_runtime::{Context, Runtime};

fn pump_until<F: Fn(&Context) -> bool>(ctx: &Context, predicate: F, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        ctx.run_pending_timers();
        if predicate(ctx) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
    }
}

#[test]
fn blob_from_strings_has_real_size_and_type() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            r#"
            const b = new Blob(['hello ', 'world'], { type: 'text/plain' });
            `${b.size}|${b.type}`
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "11|text/plain");
}

#[test]
fn blob_default_type_is_empty_string() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert_eq!(ctx.eval("new Blob(['x']).type", "<test>").unwrap(), "");
}

#[test]
fn blob_text_resolves_with_the_real_concatenated_bytes() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.seen = null;
        new Blob(['ab', 'cd']).text().then((t) => { seen = t; });
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("seen !== null", "<test>").unwrap() == "true", Duration::from_secs(2));
    assert!(done);
    assert_eq!(ctx.eval("seen", "<test>").unwrap(), "abcd");
}

#[test]
fn blob_array_buffer_resolves_with_a_real_array_buffer() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.seen = null;
        new Blob(['hi']).arrayBuffer().then((buf) => {
            seen = `${Object.prototype.toString.call(buf)}|${buf.byteLength}`;
        });
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("seen !== null", "<test>").unwrap() == "true", Duration::from_secs(2));
    assert!(done);
    assert_eq!(ctx.eval("seen", "<test>").unwrap(), "[object ArrayBuffer]|2");
}

#[test]
fn blob_slice_returns_a_new_blob_over_the_byte_range() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.seen = null;
        new Blob(['hello world']).slice(0, 5).text().then((t) => { seen = t; });
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("seen !== null", "<test>").unwrap() == "true", Duration::from_secs(2));
    assert!(done);
    assert_eq!(ctx.eval("seen", "<test>").unwrap(), "hello");
}

#[test]
fn blob_can_be_constructed_from_a_uint8array_part() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("new Blob([new Uint8Array([104, 105])]).size", "<test>")
        .unwrap();
    assert_eq!(result, "2");
}

#[test]
fn blob_can_be_constructed_from_another_blob_part() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("new Blob([new Blob(['ab']), 'cd']).size", "<test>").unwrap();
    assert_eq!(result, "4");
}

#[test]
fn file_has_name_and_extends_blob_behavior() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            r#"
            const f = new File(['content'], 'report.txt', { type: 'text/plain' });
            `${f.name}|${f.size}|${f.type}`
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "report.txt|7|text/plain");
}

#[test]
fn create_object_url_returns_a_blob_url_string() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("URL.createObjectURL(new Blob(['x'])).startsWith('blob:')", "<test>")
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn revoke_object_url_does_not_throw_on_an_unknown_url() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert!(ctx.eval("URL.revokeObjectURL('blob:does-not-exist'); true", "<test>").is_ok());
}
