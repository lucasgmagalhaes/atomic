//! Real `scrollTop`/`scrollLeft`/`scroll()`/`scrollTo()`/`scrollBy()`
//! (`ROADMAP.md` item 22). Clamping needs a real `clientHeight`/
//! `scrollHeight` snapshot - these tests push one directly via
//! `Context::set_layout_rects`/`set_scroll_extents`, the same calls a
//! real host (`profile-worker`'s `Page::render`) makes after every layout
//! pass.

use std::collections::HashMap;

use js_runtime::{Context, Rect, Runtime};

/// A 50x50 client box with 200px of real scrollable content below/right
/// of it - `scrollHeight - clientHeight = 150`, `scrollWidth - clientWidth
/// = 150`, the clamp range every test here scrolls within/past.
fn with_scrollable_div<F: FnOnce(&Context, dom::NodeId)>(f: F) {
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_layout_rects(HashMap::from([(
        div,
        Rect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
        },
    )]));
    ctx.set_scroll_extents(HashMap::from([(div, (200.0, 200.0))]));
    f(&ctx, div);
}

#[test]
fn a_never_scrolled_element_reads_zero() {
    with_scrollable_div(|ctx, _div| {
        let result = ctx
            .eval(
                "(() => { const el = document.querySelector('div'); return `${el.scrollTop},${el.scrollLeft}`; })()",
                "<test>",
            )
            .unwrap();
        assert_eq!(result, "0,0");
    });
}

#[test]
fn scrolltop_and_scrollleft_set_get_round_trip() {
    with_scrollable_div(|ctx, _div| {
        let result = ctx
            .eval(
                "(() => { const el = document.querySelector('div'); el.scrollTop = 100; el.scrollLeft = 50; return `${el.scrollTop},${el.scrollLeft}`; })()",
                "<test>",
            )
            .unwrap();
        assert_eq!(result, "100,50");
    });
}

#[test]
fn scrolltop_clamps_to_the_real_scrollable_range() {
    with_scrollable_div(|ctx, _div| {
        // scrollHeight (200) - clientHeight (50) = 150 is the real max.
        let result = ctx
            .eval(
                "(() => { const el = document.querySelector('div'); \
                 el.scrollTop = 9999; \
                 el.scrollLeft = -50; \
                 return `${el.scrollTop},${el.scrollLeft}`; })()",
                "<test>",
            )
            .unwrap();
        assert_eq!(result, "150,0");
    });
}

#[test]
fn scrollto_sets_an_absolute_position() {
    with_scrollable_div(|ctx, _div| {
        let result = ctx
            .eval(
                "(() => { const el = document.querySelector('div'); el.scrollTo(30, 40); return `${el.scrollTop},${el.scrollLeft}`; })()",
                "<test>",
            )
            .unwrap();
        assert_eq!(result, "40,30");
    });
}

#[test]
fn scrollto_accepts_a_scrolltooptions_object() {
    with_scrollable_div(|ctx, _div| {
        let result = ctx
            .eval(
                "(() => { const el = document.querySelector('div'); el.scrollTo({ top: 60, left: 20 }); return `${el.scrollTop},${el.scrollLeft}`; })()",
                "<test>",
            )
            .unwrap();
        assert_eq!(result, "60,20");
    });
}

#[test]
fn scrollby_adds_a_delta_to_the_current_position() {
    with_scrollable_div(|ctx, _div| {
        let result = ctx
            .eval(
                "(() => { const el = document.querySelector('div'); el.scrollTo(10, 10); el.scrollBy(5, -3); return `${el.scrollTop},${el.scrollLeft}`; })()",
                "<test>",
            )
            .unwrap();
        assert_eq!(result, "7,15");
    });
}

#[test]
fn scroll_is_an_alias_for_scrollto() {
    with_scrollable_div(|ctx, _div| {
        let result = ctx
            .eval(
                "(() => { const el = document.querySelector('div'); el.scroll(15, 25); return `${el.scrollTop},${el.scrollLeft}`; })()",
                "<test>",
            )
            .unwrap();
        assert_eq!(result, "25,15");
    });
}

#[test]
fn a_non_overflowing_container_clamps_scrolltop_to_zero() {
    // No `set_scroll_extents` entry at all - scrollHeight/scrollWidth
    // fall back to clientHeight/clientWidth (no overflow), so any
    // requested scroll clamps to 0, matching a real non-scrollable
    // element rejecting a nonzero scrollTop.
    let mut d = dom::Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.append_child(root, div);

    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_layout_rects(HashMap::from([(
        div,
        Rect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
        },
    )]));

    let result = ctx
        .eval(
            "(() => { const el = document.querySelector('div'); el.scrollTop = 100; return el.scrollTop; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0");
}

#[test]
fn scroll_property_types_are_real() {
    with_scrollable_div(|ctx, _div| {
        let result = ctx
            .eval(
                "(() => { const el = document.querySelector('div'); return `${typeof el.scrollTop},${typeof el.scrollLeft},${typeof el.scroll},${typeof el.scrollTo},${typeof el.scrollBy},${typeof el.scrollIntoView}`; })()",
                "<test>",
            )
            .unwrap();
        assert_eq!(result, "number,number,function,function,function,function");
    });
}

#[test]
fn scrollintoview_stays_a_no_op() {
    with_scrollable_div(|ctx, _div| {
        let result = ctx
            .eval(
                "(() => { const el = document.querySelector('div'); el.scrollIntoView(); return 'ok'; })()",
                "<test>",
            )
            .unwrap();
        assert_eq!(result, "ok");
    });
}
