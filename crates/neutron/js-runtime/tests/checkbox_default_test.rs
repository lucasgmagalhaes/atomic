use js_runtime::{Context, Runtime};

fn eval(ctx: &Context, code: &str) -> String {
    ctx.eval(code, "<test>").unwrap()
}

fn dom_with_checkbox() -> dom::Dom {
    let mut d = dom::Dom::new();
    let root = d.root();
    let form = d.create_element("form");
    d.set_attribute(form, "id", "f");
    d.append_child(root, form);
    let checkbox = d.create_element("input");
    d.set_attribute(checkbox, "id", "c");
    d.set_attribute(checkbox, "type", "checkbox");
    d.set_attribute(checkbox, "checked", "");
    d.append_child(form, checkbox);
    let unchecked = d.create_element("input");
    d.set_attribute(unchecked, "id", "u");
    d.set_attribute(unchecked, "type", "checkbox");
    d.append_child(form, unchecked);
    d
}

#[test]
fn checked_is_independent_of_default_checked() {
    let d = dom_with_checkbox();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("c");
                const before = `${el.checked},${el.defaultChecked}`;
                el.checked = false;
                const after = `${el.checked},${el.defaultChecked}`;
                return `${before}|${after}`;
            })()"#
        ),
        "true,true|false,true"
    );
}

#[test]
fn reset_restores_checkbox_checked_to_its_default() {
    let d = dom_with_checkbox();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const checked = document.getElementById("c");
                const unchecked = document.getElementById("u");
                checked.checked = false;
                unchecked.checked = true;
                document.getElementById("f").reset();
                return `${checked.checked},${unchecked.checked}`;
            })()"#
        ),
        "true,false"
    );
}
