//! Shadow DOM (`ROADMAP.md` item 38): real `attachShadow`/`.shadowRoot`/
//! `.host`/`.mode`, real DOM structure (a shadow root is excluded from
//! its host's own `children`/`childNodes` but its content is real and
//! queryable) — see `dom_bindings::shadow`'s own module doc for the
//! scope cut (no render integration, no `<slot>`).

use js_runtime::{Context, Runtime};

fn dom_with_body() -> dom::Dom {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    d
}

#[test]
fn attach_shadow_returns_a_real_shadow_root_with_correct_mode_and_host() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_body());

    let result = ctx
        .eval(
            "(() => { \
                const host = document.createElement('my-widget'); \
                document.body.appendChild(host); \
                const root = host.attachShadow({ mode: 'open' }); \
                return [root.mode, root.host === host, host.shadowRoot === root]; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "open,true,true");
}

#[test]
fn attaching_a_second_shadow_root_throws() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_body());

    let result = ctx
        .eval(
            "(() => { \
                const host = document.createElement('my-widget'); \
                host.attachShadow({ mode: 'open' }); \
                try { host.attachShadow({ mode: 'open' }); return 'no-throw'; } \
                catch (e) { return String(e); } \
            })()",
            "<test>",
        )
        .unwrap();
    assert!(result.contains("already has a shadow root"), "{result}");
}

#[test]
fn a_closed_shadow_root_is_hidden_from_the_shadow_root_getter() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_body());

    let result = ctx
        .eval(
            "(() => { \
                const host = document.createElement('my-widget'); \
                const root = host.attachShadow({ mode: 'closed' }); \
                return [host.shadowRoot === null, root.mode]; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,closed");
}

#[test]
fn shadow_root_content_is_real_and_queryable_but_excluded_from_light_dom() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_body());

    let result = ctx
        .eval(
            "(() => { \
                const host = document.createElement('my-widget'); \
                document.body.appendChild(host); \
                const root = host.attachShadow({ mode: 'open' }); \
                const inner = document.createElement('span'); \
                inner.id = 'inner'; \
                root.appendChild(inner); \
                return [ \
                    root.querySelector('#inner') === inner, \
                    host.childNodes.length \
                ]; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,0");
}

#[test]
fn an_element_inside_an_open_shadow_tree_of_a_connected_host_is_connected() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_body());

    let result = ctx
        .eval(
            "(() => { \
                const host = document.createElement('my-widget'); \
                document.body.appendChild(host); \
                const root = host.attachShadow({ mode: 'open' }); \
                const inner = document.createElement('span'); \
                root.appendChild(inner); \
                return inner.isConnected; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn attach_shadow_without_a_valid_mode_throws() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_body());

    let result = ctx
        .eval(
            "(() => { \
                const host = document.createElement('my-widget'); \
                try { host.attachShadow({}); return 'no-throw'; } \
                catch (e) { return String(e); } \
            })()",
            "<test>",
        )
        .unwrap();
    assert!(result.contains("options.mode"), "{result}");
}
