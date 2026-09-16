use js_runtime::{Context, Runtime};

#[test]
fn a_listener_attached_in_one_eval_call_survives_to_dispatch_in_a_later_separate_eval_call() {
    // Real bug this guards against: `getElementById` used to build a
    // brand-new JS object every call, so a listener attached to the
    // object from *one* `eval()` was invisible to a `dispatchEvent` from
    // a *different*, later `eval()` call on the same node id (the common
    // real-world shape: attach a listener at page load, dispatch later
    // from a real user interaction or a separate command) - `__listeners`
    // lived on the now-discarded first wrapper object, not the fresh one
    // the second `getElementById` call built. `Node` object identity is
    // now cached per real `dom::NodeId` (see `dom_bindings::get_or_create_node_object`),
    // so this must pass with the attach and the dispatch in two entirely
    // separate top-level `eval()` calls, not one expression.
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");
    d.set_text_content(p, "before");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    ctx.eval(
        "document.getElementById('greeting').addEventListener('click', () => { document.getElementById('greeting').textContent = 'clicked'; });",
        "<attach>",
    )
    .expect("attaching the listener should eval cleanly");

    let dispatched = ctx
        .eval(
            "document.getElementById('greeting').dispatchEvent('click')",
            "<dispatch>",
        )
        .expect("dispatching should eval cleanly");
    assert_eq!(
        dispatched, "true",
        "a real listener attached in an earlier eval call should still be found and called"
    );

    let text_after = ctx
        .eval("document.getElementById('greeting').textContent", "<check>")
        .expect("reading textContent should eval cleanly");
    assert_eq!(
    text_after, "clicked",
    "the listener's real mutation should be visible through yet another fresh getElementById call"
  );
}

#[test]
fn add_event_listener_and_dispatch_event_calls_the_listener() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");
    d.set_text_content(p, "before");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('greeting'); \
                el.addEventListener('click', () => { el.textContent = 'clicked'; }); \
                const dispatched = el.dispatchEvent('click'); \
                return dispatched + ',' + el.textContent; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,clicked");
}

#[test]
fn event_listeners_run_in_registration_order_and_can_be_removed_individually() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "target");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { const el = document.getElementById('target'); let log = ''; const first = () => { log += 'a'; }; const second = () => { log += 'b'; }; el.addEventListener('click', first); el.addEventListener('click', second); el.removeEventListener('click', first); el.dispatchEvent('click'); return log; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "b");
}

#[test]
fn dispatch_event_with_no_listener_is_not_canceled() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "document.getElementById('greeting').dispatchEvent('click')",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn events_expose_target_and_current_target_bubble_and_can_be_canceled() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("section");
    let child = d.create_element("button");
    d.append_child(root, parent);
    d.append_child(parent, child);
    d.set_attribute(parent, "id", "parent");
    d.set_attribute(child, "id", "child");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const parent = document.getElementById('parent'); const child = document.getElementById('child'); let log = ''; child.addEventListener('click', event => { log += `${event.type}:${event.target === child}:${event.currentTarget === child}:${event.bubbles}:${event.cancelable}`; }); parent.addEventListener('click', event => { log += `|parent:${event.target === child}:${event.currentTarget === parent}`; event.preventDefault(); }); return `${child.dispatchEvent('click')},${log}`; })()", "<test>").unwrap();
    assert_eq!(result, "false,click:true:true:true:true|parent:true:true");
}

#[test]
fn stop_propagation_prevents_later_ancestors() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let outer = d.create_element("main");
    let inner = d.create_element("section");
    let child = d.create_element("button");
    d.append_child(root, outer);
    d.append_child(outer, inner);
    d.append_child(inner, child);
    d.set_attribute(outer, "id", "outer");
    d.set_attribute(inner, "id", "inner");
    d.set_attribute(child, "id", "child");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const outer = document.getElementById('outer'); const inner = document.getElementById('inner'); const child = document.getElementById('child'); let log = ''; outer.addEventListener('click', () => { log += 'outer'; }); inner.addEventListener('click', event => { log += 'inner'; event.stopPropagation(); }); child.dispatchEvent('click'); return log; })()", "<test>").unwrap();
    assert_eq!(result, "inner");
}

#[test]
fn nested_dispatch_is_bounded_without_leaking_depth_between_events() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let button = d.create_element("button");
    d.append_child(root, button);
    d.set_attribute(button, "id", "button");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const button = document.getElementById('button'); let calls = 0; button.addEventListener('loop', () => { calls++; button.dispatchEvent('loop'); }); let limited = false; try { button.dispatchEvent('loop'); } catch (_) { limited = true; } const first = calls; button.removeEventListener('loop'); button.addEventListener('done', () => { calls++; }); const second = button.dispatchEvent('done'); return `${first},${limited},${second},${calls}`; })()", "<test>").unwrap();
    let parts: Vec<&str> = result.split(',').collect();
    let first: u32 = parts[0].parse().unwrap();
    let limited = parts[1] == "true";
    let second = parts[2] == "true";
    let calls: u32 = parts[3].parse().unwrap();
    assert!(
        first > 1,
        "recursion should have run at least twice before being bounded"
    );
    assert!(limited, "nested dispatch should have been bounded");
    assert!(second, "subsequent dispatch should succeed");
    assert_eq!(calls, first + 1);
}

#[test]
fn event_listener_limit_throws_without_registering_an_extra_callback() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let button = d.create_element("button");
    d.append_child(root, button);
    d.set_attribute(button, "id", "button");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const button = document.getElementById('button'); let calls = 0; const listener = () => { calls++; }; for (let i = 0; i < 64; i++) button.addEventListener('click', listener); let limited = false; try { button.addEventListener('click', listener); } catch (_) { limited = true; } button.dispatchEvent('click'); return `${limited},${calls}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,64");
}

#[test]
fn event_propagation_limit_throws_for_a_deep_dom_tree() {
    let mut d = dom::Dom::new();
    let mut parent = d.root();
    for _ in 0..129 {
        let child = d.create_element("div");
        d.append_child(parent, child);
        parent = child;
    }
    d.set_attribute(parent, "id", "target");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { try { document.getElementById('target').dispatchEvent('click'); return 'not-limited'; } catch (_) { return 'limited'; } })()", "<test>").unwrap();
    assert_eq!(result, "limited");
}

#[test]
fn listener_exceptions_are_rethrown_after_later_listeners_run() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let button = d.create_element("button");
    d.append_child(root, button);
    d.set_attribute(button, "id", "button");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const button = document.getElementById('button'); let log = ''; button.addEventListener('click', () => { throw new Error('expected'); }); button.addEventListener('click', () => { log += 'later'; }); try { button.dispatchEvent('click'); } catch (_) { log += ':caught'; } return log; })()", "<test>").unwrap();
    assert_eq!(result, "later:caught");
}

#[test]
fn remove_event_listener_stops_future_dispatch() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);
    d.set_attribute(p, "id", "greeting");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('greeting'); \
                let calls = 0; \
                const handler = () => { calls++; }; \
                el.addEventListener('click', handler); \
                el.removeEventListener('click'); \
                el.dispatchEvent('click'); \
                return calls; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0");
}
