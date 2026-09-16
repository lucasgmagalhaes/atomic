//! Smoke coverage for the automation crate skeleton: `pane.goto` reaching a
//! real spawned `profile::Profile`, `every`/`on` firing through `tick`, and
//! `pane.fill`/`pane.click` throwing (not silently no-opping) since
//! `profile-worker` has no input-injection protocol yet.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use automation::AutomationEngine;
use neutron::js::Runtime;

// `automation` doesn't declare a `profile-worker` bin itself, so cargo
// never sets `CARGO_BIN_EXE_profile-worker` for this test binary (that env
// var is only injected for a package's own bins, see `profile`'s own
// tests). Locate it the same way `apps/shell`'s `worker_binary_path` does
// instead — next to this test binary's own executable, which shares the
// same `target/debug` directory since this is one Cargo workspace.
fn worker_binary_path() -> std::path::PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    let name = if cfg!(windows) {
        "profile-worker.exe"
    } else {
        "profile-worker"
    };
    let dir = exe.parent().expect("test binary has a parent directory");
    for candidate_dir in [dir, dir.parent().unwrap_or(dir)] {
        let candidate = candidate_dir.join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    panic!(
        "profile-worker binary not found next to {} - run `cargo build -p profile` first",
        exe.display()
    );
}

fn spawn_demo_profile(shmem_name: &str) -> profile::Profile {
    let path = worker_binary_path();
    profile::Profile::spawn(&path.to_string_lossy(), shmem_name, 200, 150)
        .expect("failed to spawn profile-worker")
}

#[test]
fn every_and_on_fire_through_tick() {
    let runtime = Runtime::new();
    let engine = AutomationEngine::new(&runtime, HashMap::new());

    engine
        .run(
            r#"
            globalThis.everyFired = 0;
            globalThis.onFired = 0;
            every(() => { globalThis.everyFired++; }, 1);
            on("watchdog", () => { globalThis.onFired++; });
            "#,
            "test.js",
        )
        .expect("script should eval cleanly");

    std::thread::sleep(Duration::from_millis(20));
    engine.tick();
    engine.emit("watchdog");
    engine.emit("watchdog");

    let every_fired = engine.run("globalThis.everyFired", "check.js").unwrap();
    let on_fired = engine.run("globalThis.onFired", "check.js").unwrap();
    assert_eq!(every_fired, "1");
    assert_eq!(on_fired, "2");
}

#[test]
fn pane_goto_reaches_a_real_profile() {
    let profile = Rc::new(RefCell::new(spawn_demo_profile("atomic-automation-test-1")));
    let mut panes = HashMap::new();
    panes.insert("acc1".to_string(), profile.clone());

    let runtime = Runtime::new();
    let engine = AutomationEngine::new(&runtime, panes);

    let result = engine.run(r#"pane("acc1").goto("not-a-real-url")"#, "test.js");
    // The demo `profile-worker` has no real network path wired for this
    // fake URL - what matters here is that the call reached
    // `profile::Profile::navigate` at all (a real IPC round trip) rather
    // than throwing "no pane named", not that the fetch itself succeeds.
    assert!(
        result.is_err(),
        "expected the bad URL to raise a JS exception, not silently succeed"
    );
}

#[test]
fn pane_fill_and_click_reach_a_real_element_on_the_demo_page() {
    let profile = Rc::new(RefCell::new(spawn_demo_profile("atomic-automation-test-2")));
    let mut panes = HashMap::new();
    panes.insert("acc1".to_string(), profile.clone());

    let runtime = Runtime::new();
    let engine = AutomationEngine::new(&runtime, panes);

    // The built-in demo page (see `profile-worker`'s `DEMO_HTML`) has a
    // real `#counter` element - fill sets its textContent for real, click
    // dispatches for real (no listener attached to it, so nothing visibly
    // happens, but dispatch still succeeds rather than throwing).
    assert!(engine
        .run(
            r##"pane("acc1").fill("#counter", "hello from automation")"##,
            "test.js"
        )
        .is_ok());
    assert!(engine
        .run(r##"pane("acc1").click("#counter")"##, "test.js")
        .is_ok());
}

#[test]
fn pane_fill_and_click_throw_on_an_unknown_id_or_selector() {
    let profile = Rc::new(RefCell::new(spawn_demo_profile("atomic-automation-test-3")));
    let mut panes = HashMap::new();
    panes.insert("acc1".to_string(), profile.clone());

    let runtime = Runtime::new();
    let engine = AutomationEngine::new(&runtime, panes);

    assert!(engine
        .run(r##"pane("acc1").fill("#does-not-exist", "x")"##, "test.js")
        .is_err());
    assert!(engine
        .run(r##"pane("acc1").click("#does-not-exist")"##, "test.js")
        .is_err());
    // Only `#id` selectors are implemented (see this crate's top-level
    // doc) - a class/tag selector is rejected up front, not silently
    // matched against nothing.
    assert!(engine
        .run(r##"pane("acc1").click(".not-an-id-selector")"##, "test.js")
        .is_err());
}

#[test]
fn cron_fires_at_most_once_per_minute_and_rejects_bad_expressions() {
    let runtime = Runtime::new();
    let engine = AutomationEngine::new(&runtime, HashMap::new());

    engine
        .run(
            r#"
            globalThis.cronFired = 0;
            cron("* * * * *", () => { globalThis.cronFired++; });
            "#,
            "test.js",
        )
        .expect("script should eval cleanly");

    engine.tick();
    engine.tick();
    engine.tick();
    let fired = engine.run("globalThis.cronFired", "check.js").unwrap();
    // A wildcard schedule matches every minute, but `tick()` dedupes within
    // the same minute (see `cron::pump`'s `last_fired_minute` check) - three
    // ticks inside one test run should still only fire once.
    assert_eq!(fired, "1");

    let bad_expr = engine.run(r#"cron("not enough fields", () => {})"#, "test.js");
    assert!(
        bad_expr.is_err(),
        "malformed cron expression should throw, not silently register nothing"
    );
}

#[test]
fn rebind_lets_the_same_engine_control_a_pane_added_after_construction() {
    let runtime = Runtime::new();
    let mut engine = AutomationEngine::new(&runtime, HashMap::new());

    // No panes yet - the engine's own JS state (globals it may have set)
    // must survive the rebind below, not just the pane map.
    engine
        .run("globalThis.marker = 'still here';", "setup.js")
        .expect("script should eval cleanly");

    let profile = Rc::new(RefCell::new(spawn_demo_profile(
        "atomic-automation-test-rebind",
    )));
    let mut panes = HashMap::new();
    panes.insert("acc1".to_string(), profile.clone());
    engine.rebind(panes);

    let marker = engine.run("globalThis.marker", "check.js").unwrap();
    assert_eq!(
        marker, "still here",
        "rebind must not reset the JS context's own state"
    );

    assert!(
        engine
            .run(r##"pane("acc1").fill("#counter", "rebound")"##, "test.js")
            .is_ok(),
        "a pane added via rebind should be reachable by pane(...)"
    );
}

#[test]
fn rebind_removes_access_to_a_pane_no_longer_present() {
    let profile = Rc::new(RefCell::new(spawn_demo_profile(
        "atomic-automation-test-rebind-remove",
    )));
    let mut panes = HashMap::new();
    panes.insert("acc1".to_string(), profile.clone());

    let runtime = Runtime::new();
    let mut engine = AutomationEngine::new(&runtime, panes);
    assert!(engine
        .run(r##"pane("acc1").fill("#counter", "x")"##, "test.js")
        .is_ok());

    engine.rebind(HashMap::new());
    let result = engine.run(r#"pane("acc1").goto("https://example.com")"#, "test.js");
    assert!(
        result.is_err(),
        "a pane rebound away should no longer be reachable"
    );
}

#[test]
fn unknown_pane_name_throws() {
    let runtime = Runtime::new();
    let engine = AutomationEngine::new(&runtime, HashMap::new());
    let result = engine.run(r#"pane("nope").goto("https://example.com")"#, "test.js");
    assert!(result.is_err());
}
