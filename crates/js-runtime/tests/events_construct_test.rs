use js_runtime::{Context, Runtime};

#[test]
fn constructed_events_are_dispatched_by_identity() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let button = d.create_element("button");
    d.set_attribute(button, "id", "button");
    d.append_child(root, button);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const button = document.getElementById('button'); const event = new Event('save'); let seen = false; button.addEventListener('save', received => { seen = received === event && received.type === 'save' && received.target === button && received.currentTarget === button && !received.bubbles && !received.cancelable; }); return `${button.dispatchEvent(event)},${seen}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,true");
}

#[test]
fn element_event_methods_are_reinstalled_for_each_context() {
    let rt = Runtime::new();

    {
        let mut first_dom = dom::Dom::new();
        let button = first_dom.create_element("button");
        first_dom.set_attribute(button, "id", "button");
        first_dom.append_child(first_dom.root(), button);
        let first = Context::with_dom(&rt, first_dom);
        assert_eq!(
            first
                .eval(
                    "typeof document.getElementById('button').addEventListener",
                    "<test>"
                )
                .unwrap(),
            "function"
        );
    }

    let mut second_dom = dom::Dom::new();
    let button = second_dom.create_element("button");
    second_dom.set_attribute(button, "id", "button");
    second_dom.append_child(second_dom.root(), button);
    let second = Context::with_dom(&rt, second_dom);
    assert_eq!(
        second
            .eval("(() => { const button = document.getElementById('button'); let called = false; button.addEventListener('click', () => { called = true; }); button.dispatchEvent('click'); return called; })()", "<test>")
            .unwrap(),
        "true"
    );
}

#[test]
fn constructed_event_options_control_bubbling_and_cancellation() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let parent = d.create_element("section");
    let button = d.create_element("button");
    d.set_attribute(parent, "id", "parent");
    d.set_attribute(button, "id", "button");
    d.append_child(root, parent);
    d.append_child(parent, button);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const parent = document.getElementById('parent'); const button = document.getElementById('button'); let bubbled = false; parent.addEventListener('save', event => { bubbled = event.bubbles; event.preventDefault(); }); const event = new Event('save', { bubbles: true, cancelable: true, ignored: true }); return `${button.dispatchEvent(event)},${bubbled},${event.defaultPrevented}`; })()", "<test>").unwrap();
    assert_eq!(result, "false,true,true");
}
