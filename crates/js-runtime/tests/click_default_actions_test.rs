use js_runtime::{Context, Runtime};

fn eval(ctx: &Context, code: &str) -> String {
    ctx.eval(code, "<test>").unwrap()
}

fn dom_with_form_and_buttons() -> dom::Dom {
    let mut d = dom::Dom::new();
    let root = d.root();
    let form = d.create_element("form");
    d.set_attribute(form, "id", "f");
    d.append_child(root, form);
    let input = d.create_element("input");
    d.set_attribute(input, "id", "a");
    d.set_attribute(input, "value", "original");
    d.append_child(form, input);
    // No `type` attribute - a real `<button>`'s default type is "submit".
    let submit_button = d.create_element("button");
    d.set_attribute(submit_button, "id", "submit-btn");
    d.append_child(form, submit_button);
    let reset_button = d.create_element("button");
    d.set_attribute(reset_button, "id", "reset-btn");
    d.set_attribute(reset_button, "type", "reset");
    d.append_child(form, reset_button);
    d
}

#[test]
fn clicking_a_submit_button_triggers_the_forms_default_submit_action() {
    let d = dom_with_form_and_buttons();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                let submitted = false;
                document.getElementById("f").addEventListener("submit", () => { submitted = true; });
                document.getElementById("submit-btn").dispatchEvent("click");
                return String(submitted);
            })()"#
        ),
        "true"
    );
}

#[test]
fn clicking_a_reset_button_triggers_the_forms_default_reset_action() {
    let d = dom_with_form_and_buttons();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const input = document.getElementById("a");
                input.value = "typed";
                document.getElementById("reset-btn").dispatchEvent("click");
                return input.value;
            })()"#
        ),
        "original"
    );
}

#[test]
fn preventing_default_on_the_click_stops_the_submit_default_action() {
    let d = dom_with_form_and_buttons();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                let submitted = false;
                document.getElementById("f").addEventListener("submit", () => { submitted = true; });
                document.getElementById("submit-btn").addEventListener("click", (e) => { e.preventDefault(); });
                document.getElementById("submit-btn").dispatchEvent("click");
                return String(submitted);
            })()"#
        ),
        "false"
    );
}
