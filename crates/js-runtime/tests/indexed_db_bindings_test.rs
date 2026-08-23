use js_runtime::{Context, Runtime};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("nimble-idb-binding-test-{tag}-{nanos}"))
}

#[test]
fn without_storage_open_returns_null() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    assert_eq!(ctx.eval("indexedDB.open('mydb') === null", "<test>").unwrap(), "true");
}

#[test]
fn put_and_get_a_string() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("string")).unwrap();

    let result = ctx
        .eval(
            r#"
            const db = indexedDB.open('mydb');
            db.createObjectStore('profiles');
            db.put('profiles', 'p1', 'alice');
            db.get('profiles', 'p1');
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "alice");
}

#[test]
fn get_on_a_missing_key_returns_null() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("missing")).unwrap();

    let result = ctx
        .eval(
            r#"
            const db = indexedDB.open('mydb');
            db.createObjectStore('profiles');
            db.get('profiles', 'nope') === null;
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn put_and_get_a_real_structured_object() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("structured")).unwrap();

    let result = ctx
        .eval(
            r#"
            const db = indexedDB.open('mydb');
            db.createObjectStore('profiles');
            db.put('profiles', 'p1', { name: 'alice', age: 30, tags: ['admin', 'eng'], active: true, nickname: null });
            const back = db.get('profiles', 'p1');
            JSON.stringify([back.name, back.age, back.tags, back.active, back.nickname]);
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, r#"["alice",30,["admin","eng"],true,null]"#);
}

#[test]
fn delete_removes_a_key() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("delete")).unwrap();

    let result = ctx
        .eval(
            r#"
            const db = indexedDB.open('mydb');
            db.createObjectStore('s');
            db.put('s', 'k', 'v');
            db.delete('s', 'k');
            db.get('s', 'k') === null;
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn clear_empties_the_store() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("clear")).unwrap();

    let result = ctx
        .eval(
            r#"
            const db = indexedDB.open('mydb');
            db.createObjectStore('s');
            db.put('s', 'a', '1');
            db.put('s', 'b', '2');
            db.clear('s');
            JSON.stringify([db.get('s', 'a'), db.get('s', 'b')]);
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "[null,null]");
}

#[test]
fn data_persists_across_separate_context_and_open_calls() {
    let rt = Runtime::new();
    let dir = temp_dir("persist");
    {
        let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", &dir).unwrap();
        ctx.eval(
            r#"
            const db = indexedDB.open('mydb');
            db.createObjectStore('s');
            db.put('s', 'k', 'persisted-value');
            "#,
            "<test>",
        )
        .unwrap();
    }

    let ctx2 = Context::with_storage(&rt, dom::Dom::new(), "example.com", &dir).unwrap();
    let result = ctx2.eval("indexedDB.open('mydb').get('s', 'k')", "<test>").unwrap();
    assert_eq!(result, "persisted-value");
}

#[test]
fn different_database_names_are_independent() {
    let rt = Runtime::new();
    let ctx = Context::with_storage(&rt, dom::Dom::new(), "example.com", temp_dir("multi-db")).unwrap();

    let result = ctx
        .eval(
            r#"
            const a = indexedDB.open('a');
            a.createObjectStore('s');
            a.put('s', 'k', 'from-a');

            const b = indexedDB.open('b');
            b.createObjectStore('s');
            b.put('s', 'k', 'from-b');

            JSON.stringify([a.get('s', 'k'), b.get('s', 'k')]);
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, r#"["from-a","from-b"]"#);
}
