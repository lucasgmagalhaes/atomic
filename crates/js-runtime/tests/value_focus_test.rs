use js_runtime::{Context, Runtime};

#[test]
fn node_value_is_real_and_independent_of_text_content() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let initial = ctx
        .eval("document.getElementById('field').value", "<test>")
        .unwrap();
    assert_eq!(initial, "");

    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('field'); \
                el.value = 'typed'; \
                return el.value + ',' + el.textContent; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "typed,");
}

#[test]
fn textarea_value_falls_back_to_text_content_until_set() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let textarea = d.create_element("textarea");
    d.append_child(root, textarea);
    d.set_attribute(textarea, "id", "notes");
    d.set_text_content(textarea, "seeded");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let value = ctx
        .eval("document.getElementById('notes').value", "<test>")
        .unwrap();
    assert_eq!(value, "seeded");
}

#[test]
fn focus_blur_and_active_element_are_real() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let before = ctx.eval("document.activeElement", "<test>").unwrap();
    assert_eq!(before, "null");

    let after_focus = ctx
        .eval(
            "(() => { document.getElementById('field').focus(); return document.activeElement === document.getElementById('field'); })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(after_focus, "true");

    let after_blur = ctx
        .eval(
            "(() => { document.getElementById('field').blur(); return document.activeElement; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(after_blur, "null");
}

#[test]
fn focus_dispatches_a_real_focus_event() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let fired = ctx
        .eval(
            "(() => { \
                let seen = false; \
                const el = document.getElementById('field'); \
                el.addEventListener('focus', () => { seen = true; }); \
                el.focus(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(fired, "true");
}

#[test]
fn blur_dispatches_a_real_blur_event() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let fired = ctx
        .eval(
            "(() => { \
                let seen = false; \
                const el = document.getElementById('field'); \
                el.addEventListener('blur', () => { seen = true; }); \
                el.focus(); \
                el.blur(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(fired, "true");
}

#[test]
fn blur_is_a_no_op_and_does_not_dispatch_when_the_node_is_not_focused() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let fired = ctx
        .eval(
            "(() => { \
                let seen = false; \
                const el = document.getElementById('field'); \
                el.addEventListener('blur', () => { seen = true; }); \
                el.blur(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(fired, "false");
}

#[test]
fn setting_value_dispatches_a_real_input_event() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let seen_value = ctx
        .eval(
            "(() => { \
                let seenValue = null; \
                const el = document.getElementById('field'); \
                el.addEventListener('input', () => { seenValue = el.value; }); \
                el.value = 'typed'; \
                return seenValue; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(seen_value, "typed");
}

#[test]
fn blur_dispatches_a_real_change_event_only_when_value_moved_since_focus() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    d.set_attribute(input, "id", "field");

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let changed_after_edit = ctx
        .eval(
            "(() => { \
                let seen = false; \
                const el = document.getElementById('field'); \
                el.addEventListener('change', () => { seen = true; }); \
                el.focus(); \
                el.value = 'typed'; \
                el.blur(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(changed_after_edit, "true");

    let changed_without_edit = ctx
        .eval(
            "(() => { \
                let seen = false; \
                const el = document.getElementById('field'); \
                el.addEventListener('change', () => { seen = true; }); \
                el.focus(); \
                el.blur(); \
                return seen; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(changed_without_edit, "false");
}
