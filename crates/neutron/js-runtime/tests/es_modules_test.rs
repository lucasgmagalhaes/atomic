//! Real ES module linking/execution + `import()` (`ROADMAP.md` item 19).
//! Stage 4 (`module_loader_test.rs`) only proved the resolver/cache
//! primitive in isolation; these tests prove the real thing built on top
//! of it: `JS_SetModuleLoaderFunc` registered once per `Runtime`
//! (`runtime.rs`), driving both static `import`/`export` bindings
//! (`Context::eval_module`) and dynamic `import()` from a plain classic
//! script through the very same loader callback (no separate host hook
//! exists for dynamic import in this quickjs-ng version — see
//! `module_loader.rs`'s own doc).

use js_runtime::{Context, Runtime};

fn pump_until<F: Fn(&Context) -> bool>(
    ctx: &Context,
    predicate: F,
    timeout: std::time::Duration,
) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        ctx.run_pending_timers();
        if predicate(ctx) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// A real (hand-rolled) HTTP/1.1 server on loopback serving several
/// fixed `(path, body)` routes, one connection at a time, until every
/// route has been hit at least once — same convention `cors_test.rs`'s
/// own `serve_once_with_headers` already uses, extended to multiple
/// routes since real module resolution fetches more than one URL.
fn serve_routes(routes: Vec<(&'static str, &'static str)>) -> std::net::SocketAddr {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for _ in 0..routes.len() {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let body = routes
                .iter()
                .find(|(route, _)| *route == path)
                .map(|(_, body)| *body)
                .unwrap_or("");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

#[test]
fn eval_module_with_no_imports_runs_its_top_level_code() {
    // Real ES module semantics: module evaluation always returns a real
    // `Promise` (spec-mandated, to support top-level `await` even in a
    // module that has none) - unlike a classic script's own completion
    // value. `eval_module`'s useful contract is "linked and executed
    // without throwing", not a meaningful returned value - see
    // `Context::eval_module`'s own doc.
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval_module("globalThis.ran = true; 2 + 3", "<module>")
        .expect("a module with no imports must link and execute");
    assert_eq!(ctx.eval("globalThis.ran", "<test>").unwrap(), "true");
}

#[test]
fn a_real_static_import_binds_the_exported_value() {
    let addr = serve_routes(vec![
        ("/a.mjs", "export const value = 42;"),
        (
            "/main.mjs",
            "import { value } from './a.mjs'; globalThis.imported = value;",
        ),
    ]);
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval_module(
        "import { value } from './a.mjs'; globalThis.imported = value;",
        &format!("http://{addr}/main.mjs"),
    )
    .expect("static import must link and execute against the real fetched module");

    assert_eq!(ctx.eval("globalThis.imported", "<test>").unwrap(), "42");
}

#[test]
fn dynamic_import_from_a_classic_script_resolves_a_real_namespace() {
    let addr = serve_routes(vec![("/a.mjs", "export const value = 7;")]);
    let rt = Runtime::new();
    let mut ctx = Context::new(&rt);
    let main_url = format!("http://{addr}/main.js");
    ctx.set_url(&main_url);

    ctx.eval(
        "globalThis.imported = null; \
         globalThis.importError = null; \
         import('./a.mjs').then(ns => { globalThis.imported = ns.value; }, \
                                  e => { globalThis.importError = String(e); });",
        &main_url,
    )
    .expect("import() must not throw synchronously");

    let settled = pump_until(
        &ctx,
        |c| {
            c.eval("globalThis.imported", "<test>").unwrap() != "null"
                || c.eval("globalThis.importError", "<test>").unwrap() != "null"
        },
        std::time::Duration::from_secs(5),
    );
    assert!(
        settled,
        "dynamic import() must eventually settle (resolve or reject)"
    );
    assert_eq!(
        ctx.eval("globalThis.importError", "<test>").unwrap(),
        "null",
        "import() must not reject"
    );
    assert_eq!(ctx.eval("globalThis.imported", "<test>").unwrap(), "7");
}

#[test]
fn a_circular_import_pair_links_and_executes_without_hanging() {
    let addr = serve_routes(vec![
        (
            "/a.mjs",
            "import './b.mjs'; globalThis.aLoaded = true; export const fromA = 1;",
        ),
        (
            "/b.mjs",
            "import './a.mjs'; globalThis.bLoaded = true; export const fromB = 2;",
        ),
    ]);
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval_module("import './a.mjs';", &format!("http://{addr}/main.mjs"))
        .expect("a circular import pair must still link and execute, not hang or crash");

    assert_eq!(ctx.eval("globalThis.aLoaded", "<test>").unwrap(), "true");
    assert_eq!(ctx.eval("globalThis.bLoaded", "<test>").unwrap(), "true");
}

#[test]
fn a_syntax_error_in_the_module_source_reports_a_real_error() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval_module("this is not valid js at all {{{", "<module>");
    assert!(result.is_err());
}
