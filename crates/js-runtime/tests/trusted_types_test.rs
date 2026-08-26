use js_runtime::{Context, Runtime};

#[test]
fn without_the_directive_a_plain_string_is_still_accepted() {
  let rt = Runtime::new();
  let mut d = dom::Dom::new();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(d.root(), div);
  let ctx = Context::with_dom(&rt, d);

  // No set_csp call at all - the default, pre-Trusted-Types behavior.
  ctx
    .eval(
      "document.getElementById('box').innerHTML = '<b>hi</b>'",
      "<test>",
    )
    .unwrap();
  assert_eq!(
    ctx
      .eval("document.getElementById('box').innerHTML", "<test>")
      .unwrap(),
    "<b>hi</b>"
  );
}

#[test]
fn require_trusted_types_for_script_rejects_a_plain_innerhtml_string() {
  let rt = Runtime::new();
  let mut d = dom::Dom::new();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(d.root(), div);
  let mut ctx = Context::with_dom(&rt, d);
  ctx.set_csp("require-trusted-types-for 'script'");

  let result = ctx.eval(
        "(() => { try { document.getElementById('box').innerHTML = '<img src=x onerror=alert(1)>'; return 'assigned'; } catch (e) { return 'threw:' + String(e).includes('TrustedHTML'); } })()",
        "<test>",
    )
    .unwrap();
  assert_eq!(
    result, "threw:true",
    "a plain string must be rejected with a message naming TrustedHTML under enforcement"
  );

  assert_eq!(
    ctx
      .eval("document.getElementById('box').innerHTML", "<test>")
      .unwrap(),
    "",
    "the rejected assignment must not have mutated the DOM"
  );
}

#[test]
fn a_policy_created_value_is_accepted_under_enforcement() {
  let rt = Runtime::new();
  let mut d = dom::Dom::new();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(d.root(), div);
  let mut ctx = Context::with_dom(&rt, d);
  ctx.set_csp("require-trusted-types-for 'script'");

  ctx
    .eval(
      "window.p = trustedTypes.createPolicy('shout', { createHTML: (s) => s.toUpperCase() }); \
         document.getElementById('box').innerHTML = p.createHTML('<b>hi</b>');",
      "<test>",
    )
    .unwrap();

  // The policy's own transformation ran on the way in - the stored
  // element text is uppercase, proving the value flowed *through* the
  // policy rather than being accepted verbatim. (The tag itself reads
  // back lowercase: html5ever normalizes element names at parse time.)
  assert_eq!(
    ctx
      .eval("document.getElementById('box').innerHTML", "<test>")
      .unwrap(),
    "<b>HI</b>"
  );
}

#[test]
fn outerhtml_is_gated_by_the_same_directive() {
  let rt = Runtime::new();
  let mut d = dom::Dom::new();
  let parent = d.create_element("div");
  d.set_attribute(parent, "id", "parent");
  let child = d.create_element("span");
  d.set_attribute(child, "id", "box");
  d.append_child(parent, child);
  d.append_child(d.root(), parent);
  let mut ctx = Context::with_dom(&rt, d);
  ctx.set_csp("require-trusted-types-for 'script'");

  let result = ctx.eval(
        "(() => { try { document.getElementById('box').outerHTML = '<p>x</p>'; return 'assigned'; } catch (e) { return 'threw'; } })()",
        "<test>",
    )
    .unwrap();
  assert_eq!(
    result, "threw",
    "outerHTML is an HTML injection sink exactly like innerHTML"
  );
}

#[test]
fn any_delivered_policy_requiring_enforcement_tightens_it() {
  let rt = Runtime::new();
  let mut d = dom::Dom::new();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(d.root(), div);
  let mut ctx = Context::with_dom(&rt, d);
  ctx.add_csp_policy("connect-src 'none'");
  ctx.add_csp_policy("require-trusted-types-for 'script'");

  let blocked = ctx.eval(
        "(() => { try { document.getElementById('box').innerHTML = '<b>x</b>'; return false; } catch (e) { return true; } })()",
        "<test>",
    )
    .unwrap();
  assert_eq!(
    blocked, "true",
    "enforcement must be on when ANY delivered policy requires it"
  );
}

#[test]
fn replacing_the_policy_list_turns_enforcement_back_off() {
  let rt = Runtime::new();
  let mut d = dom::Dom::new();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(d.root(), div);
  let mut ctx = Context::with_dom(&rt, d);
  ctx.set_csp("require-trusted-types-for 'script'");

  // Wholesale replace with one that says nothing about trusted types -
  // same convention `set_csp` has everywhere else: the old list is gone.
  ctx.set_csp("connect-src 'self'");
  ctx
    .eval(
      "document.getElementById('box').innerHTML = '<b>ok</b>'",
      "<test>",
    )
    .unwrap();
  assert_eq!(
    ctx
      .eval("document.getElementById('box').innerHTML", "<test>")
      .unwrap(),
    "<b>ok</b>"
  );
}

#[test]
fn duplicate_policy_names_are_rejected_and_default_policy_reads_null() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  ctx
    .eval("trustedTypes.createPolicy('one', {})", "<test>")
    .unwrap();
  let duplicate = ctx.eval(
        "(() => { try { trustedTypes.createPolicy('one', {}); return false; } catch (e) { return true; } })()",
        "<test>",
    )
    .unwrap();
  assert_eq!(
    duplicate, "true",
    "real policy names are unique per document"
  );

  assert_eq!(
    ctx
      .eval("String(trustedTypes.defaultPolicy)", "<test>")
      .unwrap(),
    "null"
  );
}

#[test]
fn a_non_function_createhtml_handler_is_rejected_at_creation() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  let result = ctx.eval(
        "(() => { try { trustedTypes.createPolicy('bad', { createHTML: 42 }); return false; } catch (e) { return true; } })()",
        "<test>",
    )
    .unwrap();
  assert_eq!(result, "true");
}

#[test]
fn a_policy_without_a_handler_throws_when_called_but_creation_succeeds() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  let result = ctx.eval(
        "(() => { const p = trustedTypes.createPolicy('empty', {}); try { p.createHTML('x'); return false; } catch (e) { return true; } })()",
        "<test>",
    )
    .unwrap();
  assert_eq!(result, "true");
}

#[test]
fn trustedhtml_stringifies_to_its_data_through_the_policy() {
  let rt = Runtime::new();
  let d = dom::Dom::new();
  let ctx = Context::with_dom(&rt, d);

  let result = ctx
        .eval(
            "const p = trustedTypes.createPolicy('echo', { createHTML: (s) => s }); String(p.createHTML('<b>hi</b>'))",
            "<test>",
        )
        .unwrap();
  assert_eq!(result, "<b>hi</b>");
}
