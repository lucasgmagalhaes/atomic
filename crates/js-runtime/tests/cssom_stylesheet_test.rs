use js_runtime::{Context, Runtime};

#[test]
fn insert_rule_and_delete_rule_mutate_css_rules() {
  let d = dom::Dom::new();
  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  let result = ctx
    .eval(
      "(() => { \
                const sheet = new CSSStyleSheet(); \
                const i0 = sheet.insertRule('div { color: red; }'); \
                const i1 = sheet.insertRule('p { color: blue; }', 0); \
                const before = sheet.cssRules.length; \
                sheet.deleteRule(0); \
                const after = sheet.cssRules[0].cssText; \
                return `${i0},${i1},${before},${after}`; \
            })()",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "0,0,2,div { color: red; }");
}

#[test]
fn insert_rule_rejects_more_than_one_rule_and_empty_text() {
  let d = dom::Dom::new();
  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  let result = ctx
        .eval(
            "(() => { \
                const sheet = new CSSStyleSheet(); \
                let multiThrew = false; \
                try { sheet.insertRule('div { color: red; } p { color: blue; }'); } catch (_) { multiThrew = true; } \
                let emptyThrew = false; \
                try { sheet.insertRule(''); } catch (_) { emptyThrew = true; } \
                return `${multiThrew},${emptyThrew},${sheet.cssRules.length}`; \
            })()",
            "<test>",
        )
        .unwrap();
  assert_eq!(result, "true,true,0");
}

#[test]
fn adopted_stylesheet_text_concatenates_every_adopted_sheets_rules() {
  let d = dom::Dom::new();
  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  ctx
    .eval(
      "(() => { \
            const a = new CSSStyleSheet(); \
            a.insertRule('div { color: red; }'); \
            const b = new CSSStyleSheet(); \
            b.insertRule('p { color: blue; }'); \
            document.adoptedStyleSheets = [a, b]; \
        })()",
      "<test>",
    )
    .unwrap();
  let text = ctx.adopted_stylesheet_text();
  assert!(text.contains("div { color: red; }"), "got: {text}");
  assert!(text.contains("p { color: blue; }"), "got: {text}");
}

#[test]
fn adopted_stylesheet_text_is_empty_by_default() {
  let d = dom::Dom::new();
  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  assert_eq!(ctx.adopted_stylesheet_text(), "");
}
