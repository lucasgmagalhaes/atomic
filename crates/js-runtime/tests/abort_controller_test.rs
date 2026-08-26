use js_runtime::{Context, Runtime};

#[test]
fn abort_controller_signal_reflects_aborted_state_and_reason() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                const c = new AbortController(); \
                const before = c.signal.aborted; \
                c.abort('because'); \
                return `${before},${c.signal.aborted},${c.signal.reason}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false,true,because");
}

#[test]
fn abort_with_no_reason_uses_a_real_abort_error() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                const c = new AbortController(); \
                c.abort(); \
                return `${c.signal.reason instanceof Error},${c.signal.reason.name}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,AbortError");
}

#[test]
fn abort_fires_a_real_abort_event_on_the_signal() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                const c = new AbortController(); \
                let seen = null; \
                c.signal.addEventListener('abort', (e) => { seen = e.type; }); \
                c.abort(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "abort");
}

#[test]
fn a_second_abort_call_is_a_no_op() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                const c = new AbortController(); \
                c.abort('first'); \
                c.abort('second'); \
                return c.signal.reason; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "first");
}

#[test]
fn throw_if_aborted_throws_the_real_reason_once_aborted() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                const c = new AbortController(); \
                let before = 'no-throw'; \
                try { c.signal.throwIfAborted(); } catch (e) { before = 'threw'; } \
                c.abort('stop'); \
                let after; \
                try { c.signal.throwIfAborted(); after = 'no-throw'; } catch (e) { after = e; } \
                return `${before},${after}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "no-throw,stop");
}

#[test]
fn abort_signal_static_abort_returns_an_already_aborted_signal() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                const s = AbortSignal.abort('preset'); \
                return `${s.aborted},${s.reason}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,preset");
}

#[test]
fn new_abort_signal_throws_illegal_constructor() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                try { new AbortSignal(); return 'no-throw'; } \
                catch (e) { return 'threw'; } \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "threw");
}

#[test]
fn add_event_listener_with_an_already_aborted_signal_never_registers() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                const c = new AbortController(); \
                c.abort(); \
                let fired = false; \
                window.addEventListener('ping', () => { fired = true; }, { signal: c.signal }); \
                window.dispatchEvent(new Event('ping')); \
                return fired; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false");
}

#[test]
fn aborting_the_signal_removes_the_linked_listener() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                const c = new AbortController(); \
                let count = 0; \
                window.addEventListener('ping', () => { count++; }, { signal: c.signal }); \
                window.dispatchEvent(new Event('ping')); \
                c.abort(); \
                window.dispatchEvent(new Event('ping')); \
                return count; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1");
}

#[test]
fn a_listener_without_a_signal_is_unaffected_by_an_unrelated_abort() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                const c = new AbortController(); \
                let count = 0; \
                window.addEventListener('ping', () => { count++; }); \
                c.abort(); \
                window.dispatchEvent(new Event('ping')); \
                return count; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1");
}

#[test]
fn structured_clone_is_unaffected_by_abort_controller_registration() {
    // Sanity check that registering abort_controller alongside value_bridge
    // didn't break the other global this session touched.
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("JSON.stringify(structuredClone({a: 1}))", "<test>")
        .unwrap();
    assert_eq!(result, "{\"a\":1}");
}
