use js_runtime::{Context, Runtime};

#[test]
fn document_body_is_stable_and_accepts_dynamic_children() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let html = d.create_element("html");
    let body = d.create_element("body");
    d.append_child(root, html);
    d.append_child(html, body);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const child = document.createElement('main'); child.id = 'app'; document.body.appendChild(child); return `${document.documentElement === document.querySelector('html')},${document.body === document.querySelector('body')},${document.body.querySelector('#app') === child}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,true,true");
}

#[test]
fn class_list_is_stable_and_updates_the_class_attribute() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let element = d.create_element("div");
    d.set_attribute(element, "class", "first first");
    d.append_child(root, element);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const el = document.querySelector('div'); const list = el.classList; list.add('second'); list.remove('first'); return `${list === el.classList},${list.contains('second')},${el.className}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,true,second");
}

#[test]
fn class_list_supports_toggle_replace_value_length_and_iteration() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let element = d.create_element("div");
    d.set_attribute(element, "class", "a b");
    d.append_child(root, element);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const el = document.querySelector('div'); \
                const list = el.classList; \
                const toggledOn = list.toggle('c'); \
                const toggledOff = list.toggle('a'); \
                const forced = list.toggle('d', true); \
                const replaced = list.replace('b', 'e'); \
                const lengthAfterMutations = list.length; \
                list.value = 'x y z'; \
                const joined = [...list].join(','); \
                return `${toggledOn},${toggledOff},${forced},${replaced},${lengthAfterMutations},${list.length},${joined},${el.className}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false,true,true,3,3,x,y,z,x y z");
}

#[test]
fn dataset_reflects_live_data_attributes_by_camel_case_key() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let element = d.create_element("div");
    d.set_attribute(element, "data-user-id", "42");
    d.set_attribute(element, "data-role", "admin");
    d.append_child(root, element);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const el = document.querySelector('div'); \
                const ds = el.dataset; \
                const before = `${ds.userId},${ds.role},${ds === el.dataset}`; \
                el.removeAttribute('data-role'); \
                el.setAttribute('data-user-id', '43'); \
                return `${before},${el.dataset.userId},${el.dataset.role}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "42,admin,true,43,undefined");
}

#[test]
fn attributes_collection_is_stable_iterable_and_supports_get_named_item() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let element = d.create_element("div");
    d.set_attribute(element, "id", "panel");
    d.set_attribute(element, "data-role", "admin");
    d.append_child(root, element);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const el = document.querySelector('div'); \
                const attrs = el.attributes; \
                const stable = attrs === el.attributes; \
                const pairs = [...attrs].map(a => `${a.name}=${a.value}`).join(','); \
                const named = attrs.getNamedItem('id').value; \
                const missing = attrs.getNamedItem('nope'); \
                el.removeAttribute('data-role'); \
                return `${stable},${pairs},${named},${missing},${el.attributes.length}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,data-role=admin,id=panel,panel,null,1");
}

#[test]
fn disabled_reflects_its_attribute_while_checked_is_independent() {
    // `.disabled` is still a direct attribute reflection (real spec
    // behavior for that IDL attribute); `.checked` is now independent of
    // its content attribute, same relationship `.value`/`value` already
    // has — setting `.checked` must NOT touch the `checked` attribute
    // (that's `.defaultChecked`'s job, see `content::define_default_checked`).
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    let anchor = d.create_element("a");
    d.set_attribute(anchor, "href", "https://example.com/");
    d.append_child(root, input);
    d.append_child(root, anchor);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const input = document.querySelectorAll('input')[0]; \
                const anchor = document.querySelectorAll('a')[0]; \
                const before = `${input.checked},${input.disabled}`; \
                input.checked = true; \
                input.disabled = true; \
                const after = `${input.checked},${input.hasAttribute('checked')},${input.disabled}`; \
                input.checked = false; \
                const cleared = `${input.checked},${input.hasAttribute('checked')}`; \
                return `${before},${after},${cleared},${anchor.href}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result,
        "false,false,true,false,true,false,false,https://example.com/"
    );
}

#[test]
fn attribute_presence_and_names_follow_live_dom_attributes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let element = d.create_element("div");
    d.set_attribute(element, "data-state", "ready");
    d.set_attribute(element, "id", "panel");
    d.append_child(root, element);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const el = document.querySelector('div'); const before = `${el.hasAttribute('id')},${el.getAttributeNames().join(',')}`; el.removeAttribute('id'); return `${before},${el.hasAttribute('id')},${el.getAttributeNames().join(',')}`; })()", "<test>").unwrap();
    assert_eq!(result, "true,data-state,id,false,data-state");
}

#[test]
fn name_and_type_properties_reflect_live_attributes() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let input = d.create_element("input");
    d.append_child(root, input);
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx.eval("(() => { const input = document.querySelector('input'); input.name = 'email'; input.type = 'email'; return `${input.getAttribute('name')},${input.getAttribute('type')},${input.name},${input.type}`; })()", "<test>").unwrap();
    assert_eq!(result, "email,email,email,email");
}
