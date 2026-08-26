use js_runtime::{Context, Runtime};

fn eval(ctx: &Context, code: &str) -> String {
    ctx.eval(code, "<test>").unwrap()
}

fn dom_with_form() -> dom::Dom {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let form = d.create_element("form");
    d.set_attribute(form, "id", "f");
    d.append_child(body, form);
    let input = d.create_element("input");
    d.set_attribute(input, "id", "a");
    d.set_attribute(input, "value", "original");
    d.append_child(form, input);
    let textarea = d.create_element("textarea");
    d.set_attribute(textarea, "id", "t");
    d.set_attribute(textarea, "value", "original-t");
    d.append_child(form, textarea);
    d
}

#[test]
fn default_value_reflects_the_value_attribute_independent_of_live_value() {
    let d = dom_with_form();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("a");
                const before = `${el.value},${el.defaultValue}`;
                el.value = "typed";
                const after = `${el.value},${el.defaultValue}`;
                return `${before}|${after}`;
            })()"#
        ),
        "original,original|typed,original"
    );
}

#[test]
fn reset_restores_input_and_textarea_values_to_their_defaults() {
    let d = dom_with_form();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const input = document.getElementById("a");
                const textarea = document.getElementById("t");
                input.value = "typed";
                textarea.value = "typed-t";
                document.getElementById("f").reset();
                return `${input.value},${textarea.value}`;
            })()"#
        ),
        "original,original-t"
    );
}

#[test]
fn reset_fires_a_real_cancelable_reset_event() {
    let d = dom_with_form();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                let seen = null;
                document.getElementById("f").addEventListener("reset", (e) => { seen = e.type; });
                document.getElementById("f").reset();
                return seen;
            })()"#
        ),
        "reset"
    );
}

#[test]
fn canceling_reset_leaves_values_unchanged() {
    let d = dom_with_form();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const input = document.getElementById("a");
                input.value = "typed";
                document.getElementById("f").addEventListener("reset", (e) => e.preventDefault());
                document.getElementById("f").reset();
                return input.value;
            })()"#
        ),
        "typed"
    );
}

#[test]
fn request_submit_fires_submit_when_every_control_is_valid() {
    let d = dom_with_form();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                let seen = null;
                document.getElementById("f").addEventListener("submit", (e) => { seen = e.type; e.preventDefault(); });
                document.getElementById("f").requestSubmit();
                return seen;
            })()"#
        ),
        "submit"
    );
}

#[test]
fn request_submit_blocks_submit_and_fires_invalid_on_a_required_empty_control() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let form = d.create_element("form");
    d.set_attribute(form, "id", "f");
    d.append_child(body, form);
    let input = d.create_element("input");
    d.set_attribute(input, "id", "a");
    d.set_attribute(input, "required", "");
    d.append_child(form, input);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                let submitSeen = false;
                let invalidSeen = false;
                document.getElementById("f").addEventListener("submit", () => { submitSeen = true; });
                document.getElementById("a").addEventListener("invalid", () => { invalidSeen = true; });
                document.getElementById("f").requestSubmit();
                return `${submitSeen},${invalidSeen}`;
            })()"#
        ),
        "false,true"
    );
}
