use js_runtime::console::ConsoleLevel;
use js_runtime::{Context, Runtime};

#[test]
fn the_five_logging_methods_store_leveled_messages() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  ctx.eval(
        "console.debug('d'); console.log('l'); console.info('i'); console.warn('w'); console.error('e');",
        "<test>",
    )
    .unwrap();

  let messages = ctx.take_console_messages();
  let levels: Vec<&str> = messages.iter().map(|m| m.level.as_str()).collect();
  assert_eq!(levels, ["debug", "log", "info", "warn", "error"]);
  assert_eq!(messages[0].text, "d");
  assert_eq!(messages[4].level, ConsoleLevel::Error);
}

#[test]
fn multiple_arguments_are_space_joined_and_primitives_self_stringify() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  ctx
    .eval(
      "console.log('answer:', 42, true, null, undefined)",
      "<test>",
    )
    .unwrap();

  let messages = ctx.take_console_messages();
  assert_eq!(messages.len(), 1);
  assert_eq!(messages[0].text, "answer: 42 true null undefined");
}

#[test]
fn objects_and_arrays_are_json_inspected() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  ctx.eval("console.log({a: 1}, [1, 2])", "<test>").unwrap();

  let messages = ctx.take_console_messages();
  assert_eq!(messages[0].text, r#"{"a":1} [1,2]"#);
}

#[test]
fn unjsonable_values_fall_back_to_tostring_instead_of_throwing() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  // A cycle makes JSON.stringify throw, a top-level function isn't JSON
  // at all - both must format through their own toString, and neither
  // may leave an exception pending behind console.log's return.
  ctx
    .eval(
      "const cyc = {}; cyc.self = cyc; \
         const fn = function tagged(){}; \
         console.log(cyc, fn); \
         'formatted'",
      "<test>",
    )
    .unwrap();

  let messages = ctx.take_console_messages();
  assert_eq!(messages[0].text, "[object Object] function tagged(){}");
}

#[test]
fn taking_drains_the_buffer() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  ctx.eval("console.log('first')", "<test>").unwrap();
  assert_eq!(ctx.take_console_messages().len(), 1);
  assert!(
    ctx.take_console_messages().is_empty(),
    "a second drain must come back empty"
  );

  ctx.eval("console.log('second')", "<test>").unwrap();
  assert_eq!(ctx.take_console_messages()[0].text, "second");
}

#[test]
fn the_buffer_keeps_only_the_most_recent_messages_under_spam() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  ctx
    .eval(
      "for (let i = 0; i < 1100; i++) console.log('m' + i)",
      "<test>",
    )
    .unwrap();

  let messages = ctx.take_console_messages();
  assert_eq!(messages.len(), 1000, "the ring must cap retained messages");
  assert_eq!(
    messages[0].text, "m100",
    "oldest entries must be dropped first"
  );
  assert_eq!(messages.last().unwrap().text, "m1099");
}

#[test]
fn console_exists_and_logs_safely_even_without_a_dom_or_host_state() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);

  assert_eq!(ctx.eval("typeof console", "<test>").unwrap(), "object");
  ctx.eval("console.log('gone')", "<test>").unwrap();
  // No host state behind the buffer: nothing to drain, but no panic
  // either (same degrade-gracefully contract as cookies/localStorage).
  assert!(ctx.take_console_messages().is_empty());
}

#[test]
fn an_uncaught_error_surfaces_its_real_text() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  let err = ctx
    .eval("throw new TypeError('boom')", "<test>")
    .unwrap_err();
  assert!(
    err.0.contains("TypeError") && err.0.contains("boom"),
    "the EvalError must carry the real stringified exception, got: {}",
    err.0
  );
}

#[test]
fn a_thrown_string_is_reported_verbatim() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  let err = ctx.eval("throw 'plain failure'", "<test>").unwrap_err();
  assert_eq!(err.0, "plain failure");
}

#[test]
fn uncaught_errors_fire_a_window_error_event_carrying_the_message() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  ctx
    .eval(
      "window.addEventListener('error', (e) => { window.caught = e.message; })",
      "<test>",
    )
    .unwrap();

  assert!(ctx
    .eval("throw new RangeError('detonate')", "<test>")
    .is_err());
  let caught = ctx.eval("window.caught", "<test>").unwrap();
  assert!(
    caught.contains("RangeError") && caught.contains("detonate"),
    "the page's own error listener must observe the failure, got: {caught}"
  );
}

#[test]
fn uncaught_errors_are_also_reported_into_the_console_stream() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  assert!(ctx.eval("noSuchFunction()", "<test>").is_err());

  let messages = ctx.take_console_messages();
  assert_eq!(
    messages.len(),
    1,
    "exactly one console entry per uncaught error"
  );
  assert_eq!(messages[0].level, ConsoleLevel::Error);
  assert!(
    messages[0].text.starts_with("Uncaught "),
    "got: {}",
    messages[0].text
  );
}
