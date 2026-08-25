use std::thread::sleep;
use std::time::Duration;

use js_runtime::{Context, Runtime};

#[test]
fn set_timeout_does_not_fire_before_pump() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        "globalThis.calls = 0; setTimeout(() => { calls++; }, 0);",
        "<test>",
    )
    .unwrap();
    let calls = ctx.eval("calls", "<test>").unwrap();
    assert_eq!(calls, "0");
}

#[test]
fn set_timeout_fires_on_pump_once_delay_elapsed() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        "globalThis.calls = 0; setTimeout(() => { calls++; }, 10);",
        "<test>",
    )
    .unwrap();

    assert_eq!(ctx.run_pending_timers(), 0);
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "0");

    sleep(Duration::from_millis(20));
    assert_eq!(ctx.run_pending_timers(), 1);
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "1");

    // one-shot: a second pump doesn't fire it again
    assert_eq!(ctx.run_pending_timers(), 0);
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "1");
}

#[test]
fn set_interval_reschedules_itself_until_cleared() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        "globalThis.calls = 0; globalThis.iid = setInterval(() => { calls++; }, 5);",
        "<test>",
    )
    .unwrap();

    sleep(Duration::from_millis(10));
    ctx.run_pending_timers();
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "1");

    sleep(Duration::from_millis(10));
    ctx.run_pending_timers();
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "2");

    ctx.eval("clearInterval(iid);", "<test>").unwrap();
    sleep(Duration::from_millis(10));
    ctx.run_pending_timers();
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "2");
}

#[test]
fn clear_timeout_prevents_a_pending_timer_from_firing() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        "globalThis.calls = 0; globalThis.tid = setTimeout(() => { calls++; }, 5);",
        "<test>",
    )
    .unwrap();
    ctx.eval("clearTimeout(tid);", "<test>").unwrap();

    sleep(Duration::from_millis(10));
    assert_eq!(ctx.run_pending_timers(), 0);
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "0");
}

#[test]
fn interval_can_clear_itself_from_inside_its_own_callback() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        "globalThis.calls = 0; globalThis.iid = setInterval(() => { calls++; clearInterval(iid); }, 5);",
        "<test>",
    )
    .unwrap();

    sleep(Duration::from_millis(10));
    ctx.run_pending_timers();
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "1");

    sleep(Duration::from_millis(10));
    ctx.run_pending_timers();
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "1");
}

#[test]
fn request_animation_frame_fires_once_on_next_pump() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        "globalThis.calls = 0; requestAnimationFrame(() => { calls++; });",
        "<test>",
    )
    .unwrap();

    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "0");
    assert_eq!(ctx.run_pending_timers(), 1);
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "1");
    assert_eq!(ctx.run_pending_timers(), 0);
}

#[test]
fn cancel_animation_frame_prevents_it_from_firing() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        "globalThis.calls = 0; globalThis.fid = requestAnimationFrame(() => { calls++; });",
        "<test>",
    )
    .unwrap();
    ctx.eval("cancelAnimationFrame(fid);", "<test>").unwrap();

    assert_eq!(ctx.run_pending_timers(), 0);
    assert_eq!(ctx.eval("calls", "<test>").unwrap(), "0");
}
