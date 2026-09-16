use js_runtime::{Context, Runtime};

fn eval(ctx: &Context, code: &str) -> String {
    ctx.eval(code, "<test>").unwrap()
}

fn dom_with_select() -> dom::Dom {
    let mut d = dom::Dom::new();
    let root = d.root();
    let form = d.create_element("form");
    d.set_attribute(form, "id", "f");
    d.append_child(root, form);
    let select = d.create_element("select");
    d.set_attribute(select, "id", "s");
    d.append_child(form, select);
    let first = d.create_element("option");
    d.set_attribute(first, "value", "one");
    d.set_attribute(first, "selected", "");
    d.append_child(select, first);
    let second = d.create_element("option");
    d.set_attribute(second, "value", "two");
    d.append_child(select, second);
    d
}

#[test]
fn selected_is_independent_of_default_selected() {
    let d = dom_with_select();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const select = document.getElementById("s");
                const first = select.options[0];
                const before = `${first.selected},${first.defaultSelected}`;
                select.selectedIndex = 1;
                const after = `${first.selected},${first.defaultSelected}`;
                return `${before}|${after}`;
            })()"#
        ),
        "true,true|false,true",
        "picking a different option via selectedIndex must not rewrite defaultSelected/markup"
    );
}

#[test]
fn reset_restores_select_selection_to_its_default() {
    let d = dom_with_select();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const select = document.getElementById("s");
                select.selectedIndex = 1;
                document.getElementById("f").reset();
                return `${select.selectedIndex},${select.value}`;
            })()"#
        ),
        "0,one",
        "reset() must restore the <select>'s selection to its markup default"
    );
}
