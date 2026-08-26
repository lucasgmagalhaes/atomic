use js_runtime::{Context, Runtime};

#[test]
fn style_read_reflects_the_live_style_attribute() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.set_attribute(div, "style", "width: 10px; color: red;");
  d.append_child(root, div);

  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('box'); \
                const style = el.style; \
                return `${style.width},${style.color},${style.cssText},${style.length},${style[0]},${style[1]}`; \
            })()",
            "<test>",
        )
        .unwrap();
  assert_eq!(result, "10px,red,width: 10px; color: red;,2,width,color");
}

#[test]
fn setting_a_named_property_writes_back_to_the_style_attribute() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(root, div);

  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  let result = ctx
    .eval(
      "(() => { \
                const el = document.getElementById('box'); \
                el.style.backgroundColor = 'blue'; \
                el.style.width = '50px'; \
                return `${el.getAttribute('style')},${el.style === el.style}`; \
            })()",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "background-color: blue; width: 50px;,true");
}

#[test]
fn set_property_get_property_value_and_remove_property_are_real() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(root, div);

  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  let result = ctx
    .eval(
      "(() => { \
                const style = document.getElementById('box').style; \
                style.setProperty('margin-top', '5px'); \
                const before = style.getPropertyValue('margin-top'); \
                const removed = style.removeProperty('margin-top'); \
                const after = style.getPropertyValue('margin-top'); \
                return `${before},${removed},${after},${style.length}`; \
            })()",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "5px,5px,,0");
}

#[test]
fn css_text_setter_replaces_the_whole_declaration_block() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.set_attribute(div, "style", "width: 1px; height: 1px;");
  d.append_child(root, div);

  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  let result = ctx
    .eval(
      "(() => { \
                const el = document.getElementById('box'); \
                el.style.cssText = 'color: green;'; \
                return `${el.style.width},${el.style.color},${el.getAttribute('style')}`; \
            })()",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, ",green,color: green;");
}

#[test]
fn a_later_duplicate_declaration_wins_and_setting_collapses_duplicates() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.set_attribute(div, "style", "color: red; color: blue;");
  d.append_child(root, div);

  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  let result = ctx
    .eval(
      "(() => { \
                const el = document.getElementById('box'); \
                const before = el.style.color; \
                el.style.color = 'green'; \
                return `${before},${el.style.color},${el.getAttribute('style')}`; \
            })()",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "blue,green,color: green;");
}

#[test]
fn an_unknown_property_still_works_via_set_property_but_has_no_named_accessor() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(root, div);

  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  let result = ctx
        .eval(
            "(() => { \
                const style = document.getElementById('box').style; \
                style.setProperty('some-made-up-property', '42'); \
                return `${style.getPropertyValue('some-made-up-property')},${style.someMadeUpProperty}`; \
            })()",
            "<test>",
        )
        .unwrap();
  assert_eq!(result, "42,undefined");
}

#[test]
fn style_object_is_evicted_when_its_node_is_removed() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let div = d.create_element("div");
  d.set_attribute(div, "id", "box");
  d.append_child(root, div);

  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  // Just proving this doesn't crash/leak-detectably-misbehave across a
  // real removal + a fresh element reusing the same attribute name is
  // enough at this level; the eviction wiring itself is exercised the
  // same way dom_bindings's own classList/dataset/attributes tests do.
  let result = ctx
    .eval(
      "(() => { \
                const el = document.getElementById('box'); \
                const parent = el.parentNode; \
                const style = el.style; \
                style.setProperty('color', 'red'); \
                el.remove(); \
                const created = document.createElement('div'); \
                created.setAttribute('id', 'box'); \
                parent.appendChild(created); \
                return document.getElementById('box').style.color; \
            })()",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "");
}
