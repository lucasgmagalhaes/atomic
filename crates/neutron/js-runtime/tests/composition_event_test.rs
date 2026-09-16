use js_runtime::{Context, Runtime};

fn context_with_empty_dom(rt: &Runtime) -> Context<'_> {
    Context::with_dom(rt, dom::Dom::new())
}

#[test]
fn composition_event_carries_data_and_instanceof_chain() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new CompositionEvent('compositionupdate', { data: 'ni', bubbles: true }); return `${e.type},${e.data},${e.bubbles},${e instanceof CompositionEvent},${e instanceof Event}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "compositionupdate,ni,true,true,true");
}

#[test]
fn composition_event_without_options_defaults_data_to_empty_string() {
    let rt = Runtime::new();
    let ctx = context_with_empty_dom(&rt);
    let result = ctx
        .eval(
            "(() => { const e = new CompositionEvent('compositionstart'); return `${e.data === ''},${e.bubbles}`; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false");
}
