use js_runtime::{Context, Runtime};

#[test]
fn child_list_observation_reports_added_and_removed_nodes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let container = d.create_element("div");
    d.append_child(root, container);
    d.set_attribute(container, "id", "container");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    ctx.eval(
        "(() => { \
            globalThis.log = []; \
            const target = document.getElementById('container'); \
            const mo = new MutationObserver((records) => { \
                for (const r of records) { \
                    globalThis.log.push(r.type + ':' + r.addedNodes.length + ':' + r.removedNodes.length); \
                } \
            }); \
            mo.observe(target, { childList: true }); \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    ctx.eval(
        "document.getElementById('container').appendChild(document.createElement('span'))",
        "<test>",
    )
    .expect("appendChild should not throw");

    ctx.run_pending_timers();

    let log = ctx
        .eval("globalThis.log.join(',')", "<test>")
        .expect("reading the log should not throw");
    assert_eq!(
        log, "childList:1:0",
        "appendChild on the observed target must deliver one childList record with one added node"
    );
}

#[test]
fn attributes_observation_reports_the_old_value() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let el = d.create_element("div");
    d.append_child(root, el);
    d.set_attribute(el, "id", "el");
    d.set_attribute(el, "data-x", "before");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    ctx.eval(
        "(() => { \
            globalThis.log = []; \
            const target = document.getElementById('el'); \
            const mo = new MutationObserver((records) => { \
                for (const r of records) { \
                    globalThis.log.push(r.type + ':' + r.attributeName + ':' + r.oldValue); \
                } \
            }); \
            mo.observe(target, { attributes: true, attributeOldValue: true }); \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    ctx.eval(
        "document.getElementById('el').setAttribute('data-x', 'after')",
        "<test>",
    )
    .expect("setAttribute should not throw");

    ctx.run_pending_timers();

    let log = ctx
        .eval("globalThis.log.join(',')", "<test>")
        .expect("reading the log should not throw");
    assert_eq!(
        log, "attributes:data-x:before",
        "an attribute change must deliver its real previous value when attributeOldValue is set"
    );
}

#[test]
fn disconnect_stops_further_delivery() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let el = d.create_element("div");
    d.append_child(root, el);
    d.set_attribute(el, "id", "el");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    ctx.eval(
        "(() => { \
            globalThis.count = 0; \
            const target = document.getElementById('el'); \
            globalThis.mo = new MutationObserver(() => { globalThis.count++; }); \
            globalThis.mo.observe(target, { attributes: true }); \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    ctx.eval(
        "document.getElementById('el').setAttribute('data-x', '1')",
        "<test>",
    )
    .expect("setAttribute should not throw");
    ctx.run_pending_timers();

    ctx.eval("globalThis.mo.disconnect()", "<test>")
        .expect("disconnect should not throw");

    ctx.eval(
        "document.getElementById('el').setAttribute('data-x', '2')",
        "<test>",
    )
    .expect("setAttribute should not throw");
    ctx.run_pending_timers();

    let count = ctx
        .eval("globalThis.count", "<test>")
        .expect("reading the count should not throw");
    assert_eq!(
        count, "1",
        "no further records should be delivered after disconnect()"
    );
}

#[test]
fn a_mutation_on_an_unobserved_node_is_not_delivered() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let observed = d.create_element("div");
    let other = d.create_element("div");
    d.append_child(root, observed);
    d.append_child(root, other);
    d.set_attribute(observed, "id", "observed");
    d.set_attribute(other, "id", "other");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    ctx.eval(
        "(() => { \
            globalThis.count = 0; \
            const target = document.getElementById('observed'); \
            const mo = new MutationObserver(() => { globalThis.count++; }); \
            mo.observe(target, { attributes: true }); \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    ctx.eval(
        "document.getElementById('other').setAttribute('data-x', '1')",
        "<test>",
    )
    .expect("setAttribute should not throw");
    ctx.run_pending_timers();

    let count = ctx
        .eval("globalThis.count", "<test>")
        .expect("reading the count should not throw");
    assert_eq!(
        count, "0",
        "a mutation on a different, unobserved node must not be delivered (no subtree/global observation)"
    );
}
