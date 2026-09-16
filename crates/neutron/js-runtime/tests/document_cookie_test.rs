use js_runtime::{Context, Runtime};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("atomic-doc-cookie-test-{tag}-{nanos}"))
}

#[test]
fn plain_context_new_has_an_empty_inert_cookie_string() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert_eq!(ctx.eval("document.cookie", "<test>").unwrap(), "");
    // Writing to it shouldn't panic/error even with no jar configured.
    assert!(ctx.eval("document.cookie = 'a=1'; 'ok'", "<test>").is_ok());
}

#[test]
fn with_dom_but_no_storage_is_also_inert() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    assert_eq!(ctx.eval("document.cookie", "<test>").unwrap(), "");
}

#[test]
fn set_then_read_a_real_cookie() {
    let rt = Runtime::new();
    let ctx =
        Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("basic")).unwrap();

    ctx.eval("document.cookie = 'session=abc123';", "<test>")
        .unwrap();
    assert_eq!(
        ctx.eval("document.cookie", "<test>").unwrap(),
        "session=abc123"
    );
}

#[test]
fn multiple_cookies_are_joined_in_the_header_string() {
    let rt = Runtime::new();
    let ctx =
        Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("multi")).unwrap();

    ctx.eval("document.cookie = 'a=1';", "<test>").unwrap();
    ctx.eval("document.cookie = 'b=2';", "<test>").unwrap();

    let value = ctx.eval("document.cookie", "<test>").unwrap();
    assert!(value.contains("a=1"));
    assert!(value.contains("b=2"));
}

#[test]
fn setting_a_cookie_with_attributes_parses_them_via_the_real_set_cookie_grammar() {
    let rt = Runtime::new();
    let ctx =
        Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("attrs")).unwrap();

    // Max-Age=0 marks it already-expired, per real Set-Cookie semantics -
    // exercising the exact same `storage::cookies::parse_set_cookie` an
    // HTTP response header would go through.
    ctx.eval("document.cookie = 'a=1; Max-Age=0';", "<test>")
        .unwrap();
    assert_eq!(ctx.eval("document.cookie", "<test>").unwrap(), "");
}

#[test]
fn cookies_persist_across_separate_contexts_sharing_a_storage_dir() {
    let rt = Runtime::new();
    let dir = temp_dir("persist");
    {
        let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", &dir).unwrap();
        ctx.eval("document.cookie = 'a=1';", "<test>").unwrap();
    }
    let ctx2 = Context::with_storage(&rt, dom::Dom::new(), "example.com", &dir).unwrap();
    assert_eq!(ctx2.eval("document.cookie", "<test>").unwrap(), "a=1");
}
