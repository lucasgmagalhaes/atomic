use js_runtime::{Context, Runtime};

#[test]
fn alert_records_its_message_and_returns_undefined() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let result = ctx
        .eval("typeof alert('hello')", "<test>")
        .expect("alert should not throw");
    assert_eq!(result, "undefined");

    let messages = ctx.take_console_messages();
    assert!(
        messages.iter().any(|m| m.text == "[alert] hello"),
        "alert's message should be recorded, got {messages:?}"
    );
}

#[test]
fn confirm_returns_false_and_records_its_message() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let result = ctx
        .eval("confirm('are you sure?')", "<test>")
        .expect("confirm should not throw");
    assert_eq!(
        result, "false",
        "a headless engine has no user to click OK, so confirm() must default to false"
    );

    let messages = ctx.take_console_messages();
    assert!(messages.iter().any(|m| m.text == "[confirm] are you sure?"));
}

#[test]
fn prompt_returns_null_and_records_its_message() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let result = ctx
        .eval("String(prompt('your name?'))", "<test>")
        .expect("prompt should not throw");
    assert_eq!(
        result, "null",
        "a headless engine has no user to answer, so prompt() must default to null"
    );

    let messages = ctx.take_console_messages();
    assert!(messages.iter().any(|m| m.text == "[prompt] your name?"));
}

#[test]
fn alert_with_no_argument_coerces_to_the_string_undefined() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    ctx.eval("alert()", "<test>")
        .expect("alert should not throw");
    let messages = ctx.take_console_messages();
    assert!(messages.iter().any(|m| m.text == "[alert] undefined"));
}
