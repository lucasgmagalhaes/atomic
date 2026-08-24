use js_runtime::{Context, Runtime};

// `CustomEvent`/`KeyboardEvent`/`PointerEvent` are registered by
// `dom_bindings::register` (see `event_subclasses.rs`'s call site), which
// only runs for `Context::with_dom` — a plain `Context::new` has no DOM and
// therefore no `Event`-family globals either. Every test here uses an empty
// `dom::Dom::new()` even when it isn't otherwise exercising the DOM.
fn context_with_empty_dom(rt: &Runtime) -> Context<'_> {
    Context::with_dom(rt, dom::Dom::new())
}

#[test]
fn custom_event_carries_detail_and_instanceof_chain() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new CustomEvent('foo', { detail: 42, bubbles: true }); return `${e.type},${e.detail},${e.bubbles},${e instanceof CustomEvent},${e instanceof Event}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "foo,42,true,true,true");
}

#[test]
fn custom_event_without_options_defaults_detail_to_null() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new CustomEvent('foo'); return `${e.detail === null},${e.bubbles}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false");
}

#[test]
fn keyboard_event_carries_fields_and_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new KeyboardEvent('keydown', { key: 'a', code: 'KeyA', ctrlKey: true }); return `${e.key},${e.code},${e.ctrlKey},${e.shiftKey},${e.altKey},${e.metaKey},${e.repeat},${e.keyCode},${e instanceof Event}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "a,KeyA,true,false,false,false,false,0,true");
}

#[test]
fn pointer_event_carries_fields_and_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new PointerEvent('pointerdown', { pointerId: 7, pointerType: 'mouse', clientX: 10, clientY: 20, button: 1 }); return `${e.pointerId},${e.pointerType},${e.clientX},${e.clientY},${e.button},${e.buttons},${e instanceof Event}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "7,mouse,10,20,1,0,true");
}

#[test]
fn custom_event_dispatches_through_generic_event_target_machinery() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let button = d.create_element("button");
    d.set_attribute(button, "id", "button");
    d.append_child(root, button);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const button = document.getElementById('button'); let seen = null; button.addEventListener('foo', event => { seen = event.detail; }); const dispatched = button.dispatchEvent(new CustomEvent('foo', { detail: 'x' })); return `${dispatched},${seen}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,x");
}

#[test]
fn custom_event_without_type_argument_throws() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    assert!(ctx.eval("new CustomEvent()", "<test>").is_err());
}

#[test]
fn keyboard_event_with_non_string_coercible_type_throws() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    // A symbol can't be coerced to a string (unlike a number, which would
    // just stringify) — exercises the same `read_string` failure path as a
    // missing argument.
    assert!(ctx
        .eval("new KeyboardEvent(Symbol('x'))", "<test>")
        .is_err());
}
