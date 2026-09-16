use js_runtime::{Context, Runtime};

#[test]
fn select_options_selected_index_and_value_work_together() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let select = d.create_element("select");
    d.append_child(body, select);
    for (i, label) in ["red", "green", "blue"].iter().enumerate() {
        let option = d.create_element("option");
        if i == 1 {
            d.set_attribute(option, "selected", "");
        }
        d.set_text_content(option, label);
        d.append_child(select, option);
    }

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const s = document.querySelector('select'); return `${s instanceof HTMLSelectElement},${s.options.length},${s.selectedIndex},${s.value}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,3,1,green");
}

#[test]
fn select_selected_index_setter_moves_selection() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let select = d.create_element("select");
    d.append_child(body, select);
    for (i, label) in ["a", "b", "c"].iter().enumerate() {
        let option = d.create_element("option");
        if i == 0 {
            d.set_attribute(option, "selected", "");
        }
        d.set_text_content(option, label);
        d.append_child(select, option);
    }

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const s = document.querySelector('select'); s.selectedIndex = 2; return `${s.selectedIndex},${s.value}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,c");
}

#[test]
fn select_value_setter_selects_matching_option() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let select = d.create_element("select");
    d.append_child(body, select);
    for value in ["x", "y", "z"] {
        let option = d.create_element("option");
        d.set_attribute(option, "value", value);
        d.set_text_content(option, &format!("label-{value}"));
        d.append_child(select, option);
    }

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const s = document.querySelector('select'); s.value = 'z'; return `${s.selectedIndex},${s.value}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "2,z");
}

#[test]
fn fresh_select_without_explicit_selection_reads_empty() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let select = d.create_element("select");
    d.append_child(body, select);
    let option = d.create_element("option");
    d.set_text_content(option, "only");
    d.append_child(select, option);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { const s = document.querySelector('select'); return `${s.selectedIndex},${s.value === ''}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "-1,true");
}
