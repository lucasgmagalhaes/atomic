use js_runtime::{Context, Runtime};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("nimble-local-storage-test-{tag}-{nanos}"))
}

#[test]
fn without_storage_getitem_returns_null_and_length_is_zero() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    assert_eq!(ctx.eval("localStorage.getItem('k') === null", "<test>").unwrap(), "true");
    assert_eq!(ctx.eval("localStorage.length", "<test>").unwrap(), "0");
    // Writing without storage configured should not throw.
    assert!(ctx.eval("localStorage.setItem('k', 'v'); 'ok'", "<test>").is_ok());
}

#[test]
fn set_then_get_a_real_item() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("basic")).unwrap();

    ctx.eval("localStorage.setItem('theme', 'dark');", "<test>").unwrap();
    assert_eq!(ctx.eval("localStorage.getItem('theme')", "<test>").unwrap(), "dark");
    assert_eq!(ctx.eval("localStorage.length", "<test>").unwrap(), "1");
}

#[test]
fn remove_item_deletes_the_key() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("remove")).unwrap();

    ctx.eval("localStorage.setItem('a', '1');", "<test>").unwrap();
    ctx.eval("localStorage.removeItem('a');", "<test>").unwrap();
    assert_eq!(ctx.eval("localStorage.getItem('a') === null", "<test>").unwrap(), "true");
    assert_eq!(ctx.eval("localStorage.length", "<test>").unwrap(), "0");
}

#[test]
fn clear_empties_the_store() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("clear")).unwrap();

    ctx.eval("localStorage.setItem('a', '1'); localStorage.setItem('b', '2');", "<test>").unwrap();
    ctx.eval("localStorage.clear();", "<test>").unwrap();
    assert_eq!(ctx.eval("localStorage.length", "<test>").unwrap(), "0");
}

#[test]
fn key_returns_a_key_by_index() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("key")).unwrap();

    ctx.eval("localStorage.setItem('only', 'value');", "<test>").unwrap();
    assert_eq!(ctx.eval("localStorage.key(0)", "<test>").unwrap(), "only");
    assert_eq!(ctx.eval("localStorage.key(5) === null", "<test>").unwrap(), "true");
}

#[test]
fn local_and_session_storage_are_independent() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("independent")).unwrap();

    ctx.eval("localStorage.setItem('k', 'from-local'); sessionStorage.setItem('k', 'from-session');", "<test>").unwrap();
    assert_eq!(ctx.eval("localStorage.getItem('k')", "<test>").unwrap(), "from-local");
    assert_eq!(ctx.eval("sessionStorage.getItem('k')", "<test>").unwrap(), "from-session");
}

#[test]
fn persists_across_separate_contexts_sharing_a_storage_dir() {
    let rt = Runtime::new();
    let dir = temp_dir("persist");
    {
        let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", &dir).unwrap();
        ctx.eval("localStorage.setItem('k', 'persisted');", "<test>").unwrap();
    }
    let ctx2 = Context::with_storage(&rt, dom::Dom::new(), "example.com", &dir).unwrap();
    assert_eq!(ctx2.eval("localStorage.getItem('k')", "<test>").unwrap(), "persisted");
}
