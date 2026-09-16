use js_runtime::{Context, Runtime};

// `CustomEvent`/`KeyboardEvent`/`PointerEvent`/`MouseEvent`/`FocusEvent` are
// registered by `dom_bindings::register` (see `event_subclasses.rs`'s call
// site), which only runs for `Context::with_dom` — a plain `Context::new`
// has no DOM and therefore no `Event`-family globals either. Every test
// here uses an empty `dom::Dom::new()` even when it isn't otherwise
// exercising the DOM.
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
fn mouse_event_carries_fields_and_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new MouseEvent('click', { screenX: 1, screenY: 2, clientX: 10, clientY: 20, ctrlKey: true, button: 2 }); return `${e.screenX},${e.screenY},${e.clientX},${e.clientY},${e.ctrlKey},${e.shiftKey},${e.button},${e.buttons},${e.relatedTarget === null},${e instanceof Event}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1,2,10,20,true,false,2,0,true,true");
}

#[test]
fn mouse_event_carries_related_target_node() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let other = d.create_element("div");
    d.set_attribute(other, "id", "other");
    d.append_child(root, other);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const other = document.getElementById('other'); const e = new MouseEvent('mouseout', { relatedTarget: other }); return e.relatedTarget === other; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn focus_event_carries_related_target_and_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new FocusEvent('blur'); return `${e.relatedTarget === null},${e.bubbles},${e instanceof Event}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false,true");
}

#[test]
fn input_event_carries_fields_and_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new InputEvent('beforeinput', { data: 'a', inputType: 'insertText', isComposing: true }); return `${e.data},${e.inputType},${e.isComposing},${e instanceof Event}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "a,insertText,true,true");
}

#[test]
fn input_event_without_options_defaults_data_to_null() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new InputEvent('input'); return `${e.data === null},${e.inputType},${e.isComposing}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,,false");
}

#[test]
fn wheel_event_carries_fields_and_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new WheelEvent('wheel', { deltaX: 1, deltaY: 2, deltaZ: 3, deltaMode: 1, clientX: 10, clientY: 20 }); return `${e.deltaX},${e.deltaY},${e.deltaZ},${e.deltaMode},${e.clientX},${e.clientY},${e instanceof Event},${e instanceof WheelEvent}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1,2,3,1,10,20,true,true");
}

#[test]
fn wheel_event_without_options_defaults_to_zero() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new WheelEvent('wheel'); return `${e.deltaX},${e.deltaY},${e.deltaZ},${e.deltaMode}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,0,0,0");
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

#[test]
fn submit_event_carries_submitter_and_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new SubmitEvent('submit', { submitter: null, bubbles: true }); return `${e.submitter === null},${e.bubbles},${e instanceof Event},${e instanceof SubmitEvent}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true,true");
}

#[test]
fn submit_event_without_options_defaults_submitter_to_null() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new SubmitEvent('submit'); return `${e.submitter === null},${e.bubbles}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false");
}

#[test]
fn drag_event_carries_fields_and_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new DragEvent('dragstart', { clientX: 10, clientY: 20, button: 1, dataTransfer: null }); return `${e.clientX},${e.clientY},${e.button},${e.buttons},${e.dataTransfer === null},${e.relatedTarget === null},${e instanceof Event},${e instanceof DragEvent}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "10,20,1,0,true,true,true,true");
}

#[test]
fn drag_event_without_options_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new DragEvent('drop'); return `${e.clientX},${e.clientY},${e.button},${e.dataTransfer === null}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,0,0,true");
}

#[test]
fn touch_event_carries_fields_and_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const t = [{ identifier: 0, clientX: 5, clientY: 10 }]; const e = new TouchEvent('touchstart', { touches: t, targetTouches: t, changedTouches: t, altKey: true }); return `${e.touches.length},${e.targetTouches.length},${e.changedTouches.length},${e.altKey},${e.metaKey},${e.ctrlKey},${e.shiftKey},${e instanceof Event},${e instanceof TouchEvent}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1,1,1,true,false,false,false,true,true");
}

#[test]
fn touch_event_without_options_defaults() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new TouchEvent('touchend'); return `${e.touches === null},${e.targetTouches === null},${e.changedTouches === null},${e.altKey},${e.metaKey}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true,false,false");
}
