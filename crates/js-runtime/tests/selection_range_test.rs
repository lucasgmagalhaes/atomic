use js_runtime::{Context, Runtime};

fn eval(ctx: &Context, code: &str) -> String {
    ctx.eval(code, "<test>").unwrap()
}

fn dom_with_input(value: &str) -> dom::Dom {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.set_attribute(input, "id", "field");
    d.set_attribute(input, "value", value);
    d.append_child(root, input);
    d
}

#[test]
fn selection_start_end_and_direction_default_to_zero_zero_none() {
    let d = dom_with_input("hello");
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("field");
                return `${el.selectionStart},${el.selectionEnd},${el.selectionDirection}`;
            })()"#
        ),
        "0,0,none"
    );
}

#[test]
fn set_selection_range_moves_start_end_and_direction() {
    let d = dom_with_input("hello world");
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("field");
                el.setSelectionRange(2, 7, "backward");
                return `${el.selectionStart},${el.selectionEnd},${el.selectionDirection}`;
            })()"#
        ),
        "2,7,backward"
    );
}

#[test]
fn set_selection_range_clamps_to_the_value_length() {
    let d = dom_with_input("hi");
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("field");
                el.setSelectionRange(0, 999);
                return `${el.selectionStart},${el.selectionEnd}`;
            })()"#
        ),
        "0,2",
        "an out-of-range end must clamp to the real value length, not stay unbounded"
    );
}

#[test]
fn set_selection_range_collapses_an_inverted_range() {
    let d = dom_with_input("hello world");
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("field");
                el.setSelectionRange(8, 3);
                return `${el.selectionStart},${el.selectionEnd}`;
            })()"#
        ),
        "3,3",
        "start > end must collapse to (end, end), matching real setSelectionRange semantics"
    );
}

#[test]
fn assigning_value_resets_the_selection_to_the_end_of_the_new_text() {
    let d = dom_with_input("hello");
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("field");
                el.setSelectionRange(1, 3);
                el.value = "hi";
                return `${el.selectionStart},${el.selectionEnd}`;
            })()"#
        ),
        "2,2",
        "a fresh .value = must move the caret to the end of the new text, not keep a stale range"
    );
}

#[test]
fn selection_start_and_end_setters_move_independently() {
    let d = dom_with_input("hello world");
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("field");
                el.selectionStart = 2;
                el.selectionEnd = 5;
                el.selectionDirection = "forward";
                return `${el.selectionStart},${el.selectionEnd},${el.selectionDirection}`;
            })()"#
        ),
        "2,5,forward"
    );
}
