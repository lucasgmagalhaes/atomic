//! `customElements` (`ROADMAP.md` item 39): real `define`/`get` with
//! hyphenated-name validation, and real synchronous `connectedCallback`/
//! `disconnectedCallback` invocation on actual document connect/
//! disconnect — see `custom_elements.rs`'s own module doc for the scope
//! cut (no `instanceof`/class substitution, no `attributeChangedCallback`).

use js_runtime::{Context, Runtime};

#[test]
fn define_rejects_a_name_without_a_hyphen() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    let err = ctx
        .eval(
            "(() => { \
                try { customElements.define('foo', class {}); return 'no-throw'; } \
                catch (e) { return String(e); } \
            })()",
            "<test>",
        )
        .unwrap();
    assert!(err.contains("not a valid custom element name"), "{err}");
}

#[test]
fn define_rejects_redefinition_of_the_same_name() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    let err = ctx
        .eval(
            "(() => { \
                customElements.define('my-el', class {}); \
                try { customElements.define('my-el', class {}); return 'no-throw'; } \
                catch (e) { return String(e); } \
            })()",
            "<test>",
        )
        .unwrap();
    assert!(err.contains("already been defined"), "{err}");
}

#[test]
fn get_returns_the_defined_constructor_and_undefined_for_unknown_names() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    let result = ctx
        .eval(
            "(() => { \
                class MyEl {} \
                customElements.define('my-el', MyEl); \
                return [customElements.get('my-el') === MyEl, customElements.get('not-defined') === undefined]; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true");
}

fn dom_with_body() -> dom::Dom {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    d
}

#[test]
fn connected_callback_fires_when_a_matching_element_is_appended() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_body());

    let result = ctx
        .eval(
            "(() => { \
                globalThis.log = []; \
                class MyEl { connectedCallback() { globalThis.log.push('connected'); } } \
                customElements.define('my-el', MyEl); \
                const el = document.createElement('my-el'); \
                document.body.appendChild(el); \
                return globalThis.log.join(','); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "connected");
}

#[test]
fn disconnected_callback_fires_on_removal_and_not_on_a_never_connected_node() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_body());

    let result = ctx
        .eval(
            "(() => { \
                globalThis.log = []; \
                class MyEl { disconnectedCallback() { globalThis.log.push('disconnected'); } } \
                customElements.define('my-el', MyEl); \
                const el = document.createElement('my-el'); \
                el.remove(); \
                const before = globalThis.log.length; \
                document.body.appendChild(el); \
                el.remove(); \
                return [before, globalThis.log.length]; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,1");
}

#[test]
fn defining_a_name_upgrades_already_connected_matching_elements() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_body());

    // The element is created and connected *before* the class is defined
    // - the common real-world order (HTML parsed first, script defining
    // the class runs later).
    let result = ctx
        .eval(
            "(() => { \
                globalThis.log = []; \
                const el = document.createElement('my-el'); \
                document.body.appendChild(el); \
                class MyEl { connectedCallback() { globalThis.log.push('upgraded'); } } \
                customElements.define('my-el', MyEl); \
                return globalThis.log.join(','); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "upgraded");
}

#[test]
fn a_non_function_constructor_is_rejected() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    let err = ctx
        .eval(
            "(() => { \
                try { customElements.define('my-el', {}); return 'no-throw'; } \
                catch (e) { return String(e); } \
            })()",
            "<test>",
        )
        .unwrap();
    assert!(err.contains("constructor must be a function"), "{err}");
}
