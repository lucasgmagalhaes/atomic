use js_runtime::{Context, Runtime};

fn ctx_with_body(rt: &Runtime) -> Context<'_> {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    Context::with_dom(rt, d)
}

#[test]
fn document_write_appends_parsed_html_into_the_body() {
    let rt = Runtime::new();
    let ctx = ctx_with_body(&rt);
    let result = ctx
        .eval(
            "(() => { document.write('<div id=\"a\">hi</div>'); return document.body.querySelector('#a').textContent; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "hi");
}

#[test]
fn document_write_concatenates_every_argument() {
    let rt = Runtime::new();
    let ctx = ctx_with_body(&rt);
    let result = ctx
        .eval(
            "(() => { document.write('<p id=\"b\">', 'foo', 'bar', '</p>'); return document.body.querySelector('#b').textContent; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "foobar");
}

#[test]
fn document_writeln_appends_a_trailing_newline() {
    let rt = Runtime::new();
    let ctx = ctx_with_body(&rt);
    let result = ctx
        .eval(
            "(() => { document.writeln('<pre id=\"c\">line'); return document.body.querySelector('#c').textContent; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "line\n");
}

#[test]
fn a_second_document_write_call_appends_after_the_first_rather_than_replacing_it() {
    let rt = Runtime::new();
    let ctx = ctx_with_body(&rt);
    let result = ctx
        .eval(
            "(() => { \
                document.write('<div id=\"first\"></div>'); \
                document.write('<div id=\"second\"></div>'); \
                return `${!!document.body.querySelector('#first')},${!!document.body.querySelector('#second')}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(
        result, "true,true",
        "document.write must not implicitly erase what was already written - this engine's own documented deviation from real spec's implicit open()"
    );
}

#[test]
fn document_write_falls_back_to_the_document_root_when_there_is_no_body() {
    let rt = Runtime::new();
    let d = dom::Dom::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { document.write('<div id=\"x\">y</div>'); const el = document.getElementById('x'); return el ? el.textContent : null; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "y");
}
