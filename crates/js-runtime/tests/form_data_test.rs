use js_runtime::{Context, Runtime};

fn eval(ctx: &Context, code: &str) -> String {
  ctx.eval(code, "<test>").unwrap()
}

#[test]
fn formdata_global_exists_and_constructs_empty() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  assert_eq!(eval(&ctx, "typeof FormData"), "function");
  assert_eq!(
    eval(
      &ctx,
      "(() => { const fd = new FormData(); return fd.entries().length; })()"
    ),
    "0"
  );
}

#[test]
fn append_get_has_delete_roundtrip() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData();
                fd.append("user", "alice");
                fd.append("age", "30");
                return `${fd.has("user")},${fd.has("nope")},${fd.get("user")},${fd.get("age")}`;
            })()"#
    ),
    "true,false,alice,30"
  );
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData();
                fd.append("x", "1");
                fd.delete("x");
                return `${fd.has("x")},${fd.get("x")}`;
            })()"#
    ),
    "false,null"
  );
}

#[test]
fn get_returns_null_for_missing_and_first_for_duplicates() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData();
                fd.append("t", "first");
                fd.append("t", "second");
                return `${fd.get("t")},${fd.getAll("t").join("|")},${fd.getAll("missing").length}`;
            })()"#
    ),
    "first,first|second,0"
  );
}

#[test]
fn set_replaces_all_existing_entries_with_the_name() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData();
                fd.append("t", "a");
                fd.append("other", "keep");
                fd.append("t", "b");
                fd.set("t", "only");
                return `${fd.getAll("t").join(",")},${fd.get("other")},${fd.entries().length}`;
            })()"#
    ),
    "only,keep,2"
  );
}

#[test]
fn entries_keys_values_return_arrays_of_pairs() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData();
                fd.append("a", "1");
                fd.append("b", "2");
                const pairs = fd.entries().map(p => p[0] + "=" + p[1]);
                return `${pairs.join(";")},${fd.keys().join(",")},${fd.values().join(",")}`;
            })()"#
    ),
    "a=1;b=2,a,b,1,2"
  );
}

#[test]
fn foreach_walks_name_value_pairs_in_order() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData();
                fd.append("one", "1");
                fd.append("two", "2");
                const seen = [];
                fd.forEach((value, name, self) => seen.push(name + ":" + value + "@" + (self === fd)));
                return seen.join(",");
            })()"#
    ),
    "one:1@true,two:2@true"
  );
}

#[test]
fn appending_a_file_stores_a_real_blob_with_name() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData();
                const file = new File(["hello"], "notes.txt", { type: "text/plain" });
                fd.append("doc", file);
                const out = fd.get("doc");
                const bytes = [];
                // text() is async; check the synchronous surface instead
                return `${out instanceof Blob},${out.name},${out.type},${out.size}`;
            })()"#
    ),
    "true,notes.txt,text/plain,5"
  );
}

#[test]
fn append_with_explicit_filename_overrides_the_files_own_name() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData();
                fd.append("doc", new Blob(["zz"]), "renamed.bin");
                return fd.get("doc").name;
            })()"#
    ),
    "renamed.bin"
  );
}

#[test]
fn plain_string_values_are_not_blobs() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData();
                fd.append("s", "just a string");
                const v = fd.get("s");
                return `${v === "just a string"},${v instanceof Blob}`;
            })()"#
    ),
    "true,false"
  );
}

#[test]
fn constructor_populates_from_a_form_element() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let body = d.create_element("body");
  d.append_child(root, body);
  let form = d.create_element("form");
  d.set_attribute(form, "id", "signup");
  d.append_child(body, form);
  let user = d.create_element("input");
  d.set_attribute(user, "type", "text");
  d.set_attribute(user, "name", "user");
  d.set_attribute(user, "value", "alice");
  d.append_child(form, user);
  let remember = d.create_element("input");
  d.set_attribute(remember, "type", "checkbox");
  d.set_attribute(remember, "name", "remember");
  d.set_attribute(remember, "checked", "");
  d.append_child(form, remember);
  let opt_in = d.create_element("input");
  d.set_attribute(opt_in, "type", "checkbox");
  d.set_attribute(opt_in, "name", "optin");
  d.append_child(form, opt_in);
  let disabled_field = d.create_element("input");
  d.set_attribute(disabled_field, "name", "locked");
  d.set_attribute(disabled_field, "value", "nope");
  d.set_attribute(disabled_field, "disabled", "");
  d.append_child(form, disabled_field);
  let unnamed = d.create_element("input");
  d.set_attribute(unnamed, "value", "noname");
  d.append_child(form, unnamed);

  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData(document.getElementById("signup"));
                return `${fd.get("user")},${fd.get("remember")},${fd.has("optin")},${fd.has("locked")}`;
            })()"#
    ),
    "alice,on,false,false"
  );
}

#[test]
fn constructor_reads_selected_select_option_and_textarea_fallback() {
  let mut d = dom::Dom::new();
  let root = d.root();
  let body = d.create_element("body");
  d.append_child(root, body);
  let form = d.create_element("form");
  d.set_attribute(form, "id", "prefs");
  d.append_child(body, form);
  let select = d.create_element("select");
  d.set_attribute(select, "name", "color");
  d.append_child(form, select);
  for value in ["red", "green"] {
    let option = d.create_element("option");
    d.set_attribute(option, "value", value);
    if value == "green" {
      d.set_attribute(option, "selected", "");
    }
    d.append_child(select, option);
  }
  let textarea = d.create_element("textarea");
  d.set_attribute(textarea, "name", "bio");
  d.set_text_content(textarea, "hello world");
  d.append_child(form, textarea);

  let rt = Runtime::new();
  let ctx = Context::with_dom(&rt, d);
  assert_eq!(
    eval(
      &ctx,
      r#"(() => {
                const fd = new FormData(document.getElementById("prefs"));
                return `${fd.get("color")},${fd.get("bio")}`;
            })()"#
    ),
    "green,hello world"
  );
}

#[test]
fn constructor_without_dom_degrades_to_empty() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  // A non-node argument is treated as no form at all.
  assert_eq!(
    eval(
      &ctx,
      "(() => { const fd = new FormData({ not: \"a node\" }); return fd.entries().length; })()"
    ),
    "0"
  );
}
