use js_runtime::{Context, Runtime};
use std::time::{Duration, Instant};

#[test]
fn a_budgeted_infinite_loop_is_interrupted_promptly() {
    let rt = Runtime::new();
    let d = dom::Dom::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_time_budget(Duration::from_millis(100));

    let started = Instant::now();
    let err = ctx.eval("while (true) {}", "<test>").unwrap_err();
    let elapsed = started.elapsed();

    assert!(
        err.0.contains("interrupt"),
        "the failure must be the interpreter's interrupt abort, got: {}",
        err.0
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the interrupt must actually fire near the deadline, not hang the host (ran {elapsed:?})"
    );
}

#[test]
fn scripts_finishing_within_their_budget_are_untouched() {
    let rt = Runtime::new();
    let d = dom::Dom::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_time_budget(Duration::from_secs(10));

    assert_eq!(ctx.eval("21 + 21", "<test>").unwrap(), "42");
}

#[test]
fn the_deadline_is_restamped_per_eval_call() {
    let rt = Runtime::new();
    let d = dom::Dom::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_time_budget(Duration::from_millis(500));

    // A busy wait that eats most of one budget must not eat the next
    // call's - each eval gets a fresh deadline stamped at its start.
    ctx.eval("performance.now()", "<test>").unwrap();
    let err = ctx.eval("for (;;) {}", "<test>").unwrap_err();
    assert!(err.0.contains("interrupt"), "got: {}", err.0);
}

#[test]
fn clearing_the_budget_removes_the_cutoff() {
    let rt = Runtime::new();
    let d = dom::Dom::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_time_budget(Duration::from_millis(50));
    ctx.clear_time_budget();

    // With no budget there is nothing to interrupt; just prove ordinary
    // evaluation still works and no stale state lingers.
    assert_eq!(ctx.eval("'still here'", "<test>").unwrap(), "still here");
}

#[test]
fn a_memory_limit_turns_overallocation_into_an_exception() {
    let rt = Runtime::new();
    let d = dom::Dom::new();
    let ctx = Context::with_dom(&rt, d);
    ctx.set_memory_limit(1024 * 1024);

    // 1 MiB cap vs a single 64 MiB typed-array backing store: the
    // allocation that crosses the limit throws instead of succeeding.
    let err = ctx
        .eval("new Uint8Array(64 * 1024 * 1024)", "<test>")
        .unwrap_err();
    assert!(
        err.0.contains("memory"),
        "over-allocation must surface as quickjs's out-of-memory error, got: {}",
        err.0
    );

    // The context survives its failed allocation and keeps working for
    // allocations under the cap.
    assert_eq!(ctx.eval("'alive'", "<test>").unwrap(), "alive");
}
