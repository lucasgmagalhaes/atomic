use js_runtime::{Context, Runtime};

fn eval(ctx: &Context, code: &str) -> String {
    ctx.eval(code, "<test>").unwrap()
}

#[test]
fn validity_object_has_all_ten_flags_and_valid_reflects_state() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let input = d.create_element("input");
    d.set_attribute(input, "id", "a");
    d.append_child(body, input);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const v = document.getElementById("a").validity;
                return `${v.valueMissing},${v.typeMismatch},${v.patternMismatch},${v.rangeUnderflow},${v.rangeOverflow},${v.stepMismatch},${v.tooLong},${v.tooShort},${v.customError},${v.valid}`;
            })()"#
        ),
        "false,false,false,false,false,false,false,false,false,true"
    );
}

#[test]
fn required_empty_input_reports_valuemissing_and_invalid() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let input = d.create_element("input");
    d.set_attribute(input, "id", "a");
    d.set_attribute(input, "required", "");
    d.append_child(body, input);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("a");
                return `${el.validity.valueMissing},${el.checkValidity()}`;
            })()"#
        ),
        "true,false"
    );
    // Filling it in clears valueMissing.
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("a");
                el.value = "now full";
                return `${el.validity.valueMissing},${el.checkValidity()}`;
            })()"#
        ),
        "false,true"
    );
}

#[test]
fn check_validity_fires_a_real_invalid_event_when_invalid() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let form = d.create_element("form");
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
                let fired = 0;
                document.getElementById("a").addEventListener("invalid", () => { fired += 1; });
                document.getElementById("a").checkValidity();
                return fired;
            })()"#
        ),
        "1"
    );
}

#[test]
fn min_max_length_flags_use_character_counts_on_non_empty_values() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let short_field = d.create_element("input");
    d.set_attribute(short_field, "id", "s");
    d.set_attribute(short_field, "minlength", "5");
    d.set_attribute(short_field, "value", "abc");
    d.append_child(body, short_field);
    let long_field = d.create_element("input");
    d.set_attribute(long_field, "id", "l");
    d.set_attribute(long_field, "maxlength", "2");
    d.set_attribute(long_field, "value", "abcdef");
    d.append_child(body, long_field);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(&ctx, "document.getElementById('s').checkValidity()"),
        "false"
    );
    assert_eq!(
        eval(&ctx, "document.getElementById('s').validity.tooShort"),
        "true"
    );
    assert_eq!(
        eval(&ctx, "document.getElementById('l').validity.tooLong"),
        "true"
    );
}

#[test]
fn set_custom_validity_drives_customerror_validationmessage_and_clearing() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let input = d.create_element("input");
    d.set_attribute(input, "id", "a");
    d.append_child(body, input);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("a");
                el.setCustomValidity("must match the secret code");
                return `${el.validity.customError},${el.validity.valid},${el.validationMessage}`;
            })()"#
        ),
        "true,false,must match the secret code"
    );
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const el = document.getElementById("a");
                el.setCustomValidity("");
                return `${el.validity.customError},${el.validity.valid},${el.validationMessage}`;
            })()"#
        ),
        "false,true,"
    );
}

#[test]
fn will_validate_is_true_for_enabled_controls_only() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let input = d.create_element("input");
    d.set_attribute(input, "id", "live");
    d.append_child(body, input);
    let disabled_input = d.create_element("input");
    d.set_attribute(disabled_input, "id", "dead");
    d.set_attribute(disabled_input, "disabled", "");
    d.append_child(body, disabled_input);
    let div = d.create_element("div");
    d.set_attribute(div, "id", "notcontrol");
    d.append_child(body, div);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                return `${document.getElementById("live").willValidate},${document.getElementById("dead").willValidate},${document.getElementById("notcontrol").willValidate}`;
            })()"#
        ),
        "true,false,false"
    );
}

#[test]
fn labels_collects_for_matches_and_wrapping_ancestors() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let explicit_label = d.create_element("label");
    d.set_attribute(explicit_label, "for", "target");
    d.append_child(body, explicit_label);
    let wrapper_label = d.create_element("label");
    d.append_child(body, wrapper_label);
    let target = d.create_element("input");
    d.set_attribute(target, "id", "target");
    d.append_child(wrapper_label, target);
    let unrelated = d.create_element("label");
    d.set_attribute(unrelated, "for", "other");
    d.append_child(body, unrelated);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const labels = document.getElementById("target").labels;
                const tags = [];
                for (let i = 0; i < labels.length; i++) tags.push(labels.item(i).tagName);
                return `${labels.length},${tags.join(",")}`;
            })()"#
        ),
        "2,LABEL,LABEL"
    );
}

#[test]
fn html_for_reflects_the_for_attribute_both_ways() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let label = d.create_element("label");
    d.set_attribute(label, "for", "x");
    d.append_child(body, label);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    assert_eq!(eval(&ctx, "document.querySelector('label').htmlFor"), "x");
    assert_eq!(
        eval(
            &ctx,
            r#"(() => {
                const l = document.querySelector("label");
                l.htmlFor = "y";
                return l.getAttribute("for");
            })()"#
        ),
        "y"
    );
}
