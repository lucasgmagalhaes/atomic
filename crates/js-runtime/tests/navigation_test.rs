use js_runtime::{Context, Runtime};

#[test]
fn window_is_the_global_object_and_aliases_self_top_parent() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                globalThis.marker = 42; \
                return `${window.marker},${window === self},${window === top},${window === parent},${typeof document}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "42,true,true,true,object");
}

#[test]
fn window_add_event_listener_and_dispatch_event_work_directly_on_the_global() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "(() => { \
                let seen = null; \
                window.addEventListener('ping', (e) => { seen = e.type; }); \
                const notCanceled = window.dispatchEvent('ping'); \
                return `${seen},${notCanceled}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "ping,true");
}

#[test]
fn location_reflects_the_url_set_via_set_url() {
    let d = dom::Dom::new();
    let _ = d.root();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://example.com:8080/path/page.html?q=1#frag");

    let result = ctx
        .eval(
            "`${location.href},${location.protocol},${location.host},${location.hostname},${location.port},${location.pathname},${location.search},${location.hash},${location.origin},${location === document.location},${location.toString() === location.href}`",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result,
        "https://example.com:8080/path/page.html?q=1#frag,https:,example.com:8080,example.com,8080,/path/page.html,?q=1,#frag,https://example.com:8080,true,true"
    );
}

#[test]
fn location_is_inert_without_a_set_url() {
    let d = dom::Dom::new();
    let _ = d.root();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "`${location.href},${location.protocol},${location.host},${location.pathname}`",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, ",,,");
}

#[test]
fn dispatch_lifecycle_events_fires_dom_content_loaded_and_load() {
    let d = dom::Dom::new();
    let _ = d.root();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    ctx.eval(
        "(() => { \
            window.__log = []; \
            document.addEventListener('DOMContentLoaded', () => { window.__log.push('dcl'); }); \
            window.addEventListener('load', () => { window.__log.push('load'); }); \
        })()",
        "<test>",
    )
    .unwrap();

    ctx.dispatch_lifecycle_events();

    let result = ctx.eval("window.__log.join(',')", "<test>").unwrap();
    assert_eq!(result, "dcl,load");
}

#[test]
fn history_push_state_updates_state_length_and_location() {
    let d = dom::Dom::new();
    let _ = d.root();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://example.com/start");

    let result = ctx
        .eval(
            "(() => { \
                const before = `${history.length},${history.state}`; \
                history.pushState({page: 1}, '', '/next'); \
                const after = `${history.length},${JSON.stringify(history.state)},${location.pathname}`; \
                return `${before}|${after}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1,null|2,{\"page\":1},/next");
}

#[test]
fn history_replace_state_does_not_grow_the_stack() {
    let d = dom::Dom::new();
    let _ = d.root();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://example.com/start");

    let result = ctx
        .eval(
            "(() => { \
                history.pushState({a: 1}, '', '/a'); \
                history.replaceState({a: 2}, '', '/a2'); \
                return `${history.length},${JSON.stringify(history.state)},${location.pathname}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,{\"a\":2},/a2");
}

#[test]
fn history_state_is_a_real_snapshot_not_a_live_reference() {
    // Mutating the object passed to pushState afterward must not be
    // visible through history.state - proves `state` is really structured
    // cloned (crate::value_bridge::deep_clone), not JS_DupValue'd.
    let d = dom::Dom::new();
    let _ = d.root();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://example.com/start");

    let result = ctx
        .eval(
            "(() => { \
                const obj = {page: 1}; \
                history.pushState(obj, '', '/next'); \
                obj.page = 999; \
                obj.extra = 'added-after'; \
                return JSON.stringify(history.state); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "{\"page\":1}");
}

#[test]
fn structured_clone_produces_an_independent_deep_copy() {
    let d = dom::Dom::new();
    let _ = d.root();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const original = {a: 1, nested: {b: [1, 2, 3]}}; \
                const clone = structuredClone(original); \
                clone.a = 999; \
                clone.nested.b.push(4); \
                return JSON.stringify({original, clone}); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result,
        "{\"original\":{\"a\":1,\"nested\":{\"b\":[1,2,3]}},\"clone\":{\"a\":999,\"nested\":{\"b\":[1,2,3,4]}}}"
    );
}

#[test]
fn history_back_forward_move_the_stack_and_fire_popstate() {
    let d = dom::Dom::new();
    let _ = d.root();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://example.com/start");

    ctx.eval(
        "(() => { \
            window.__pops = []; \
            window.addEventListener('popstate', (e) => { window.__pops.push(JSON.stringify(e.state)); }); \
            history.pushState({p: 1}, '', '/one'); \
            history.pushState({p: 2}, '', '/two'); \
        })()",
        "<test>",
    )
    .unwrap();

    let after_back = ctx
        .eval(
            "(() => { history.back(); return `${location.pathname},${JSON.stringify(history.state)}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(after_back, "/one,{\"p\":1}");

    let after_forward = ctx
        .eval(
            "(() => { history.forward(); return `${location.pathname},${JSON.stringify(history.state)}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(after_forward, "/two,{\"p\":2}");

    let pops = ctx.eval("window.__pops.join('|')", "<test>").unwrap();
    assert_eq!(pops, "{\"p\":1}|{\"p\":2}");
}

#[test]
fn history_go_beyond_the_stack_is_a_no_op() {
    let d = dom::Dom::new();
    let _ = d.root();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://example.com/start");

    let result = ctx
        .eval(
            "(() => { \
                let pops = 0; \
                window.addEventListener('popstate', () => { pops++; }); \
                history.go(-5); \
                history.go(5); \
                return `${location.pathname},${pops}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "/start,0");
}

#[test]
fn fire_before_unload_returns_true_with_no_listener() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert!(ctx.fire_before_unload());
}

#[test]
fn fire_before_unload_returns_false_when_canceled() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        "window.addEventListener('beforeunload', (e) => { e.preventDefault(); });",
        "<test>",
    )
    .unwrap();
    assert!(!ctx.fire_before_unload());
}

#[test]
fn fire_before_unload_is_true_when_a_listener_runs_but_does_not_cancel() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        "(() => { window.__ran = false; window.addEventListener('beforeunload', () => { window.__ran = true; }); })()",
        "<test>",
    )
    .unwrap();
    assert!(ctx.fire_before_unload());
    let ran = ctx.eval("window.__ran", "<test>").unwrap();
    assert_eq!(ran, "true");
}
