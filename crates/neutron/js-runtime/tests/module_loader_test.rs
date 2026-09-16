//! ES Module resolver + cache primitive (`ROADMAP.md` item 19,
//! `.claude/plans/architecture-p5-foundations.plan.md` Stage 4). No real
//! `import`/`import()` JS API exists yet — these tests exercise the
//! Rust-level resolver and fetch-and-cache primitive directly, the same
//! way `window_registry_test.rs` proves Stage 3's primitive without a
//! JS-facing `postMessage`.

use js_runtime::{Context, ModuleStatus, Runtime};

#[test]
fn resolves_a_relative_specifier_against_the_importing_modules_url() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    let resolved = ctx
        .resolve_module_specifier("https://example.com/app/main.mjs", "./utils.mjs")
        .expect("a relative specifier against a valid base must resolve");
    assert_eq!(resolved, "https://example.com/app/utils.mjs");
}

#[test]
fn resolves_an_absolute_specifier_regardless_of_base() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    let resolved = ctx
        .resolve_module_specifier(
            "https://example.com/app/main.mjs",
            "https://cdn.example.com/lib.mjs",
        )
        .expect("an absolute specifier must resolve to itself");
    assert_eq!(resolved, "https://cdn.example.com/lib.mjs");
}

#[test]
fn an_unparseable_base_fails_to_resolve() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    assert!(ctx
        .resolve_module_specifier("not a url", "./utils.mjs")
        .is_none());
}

#[test]
fn a_url_never_loaded_has_no_status() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert!(ctx
        .module_status("https://example.com/never-requested.mjs")
        .is_none());
}

#[test]
fn loading_an_unreachable_host_eventually_fails_not_hangs() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    // A reserved, non-routable address - the background fetch must
    // complete (with an error) rather than the cache staying Loading
    // forever once pumped enough times.
    let url = "http://127.0.0.1:1/unreachable.mjs";
    ctx.load_module(url);
    assert!(matches!(
        ctx.module_status(url),
        Some(ModuleStatus::Loading)
    ));

    let mut attempts = 0;
    loop {
        ctx.run_pending_timers();
        match ctx.module_status(url) {
            Some(ModuleStatus::Loading) if attempts < 300 => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Some(ModuleStatus::Failed(_)) => break,
            other => panic!("expected an eventual Failed status, got {other:?}"),
        }
    }
}

#[test]
fn loading_the_same_url_twice_does_not_duplicate_pending_work() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let url = "http://127.0.0.1:1/unreachable.mjs";
    ctx.load_module(url);
    ctx.load_module(url); // must be a no-op, not a second fetch
    assert!(matches!(
        ctx.module_status(url),
        Some(ModuleStatus::Loading)
    ));
}
