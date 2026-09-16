//! `ctx.createPattern()` - `crate::canvas_bindings::pattern::create_pattern`.
use js_runtime::{Context, Runtime};

fn context_with_two_canvases() -> (Runtime, dom::Dom) {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    for _ in 0..2 {
        let canvas = d.create_element("canvas");
        d.set_attribute(canvas, "width", "2");
        d.set_attribute(canvas, "height", "2");
        d.append_child(body, canvas);
    }
    (Runtime::new(), d)
}

#[test]
fn pattern_tiles_a_canvas_source_across_a_larger_fill_rect() {
    let (rt, d) = context_with_two_canvases();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const canvases = document.querySelectorAll('canvas'); \
               const src = canvases[0].getContext('2d'); \
               src.fillStyle = '#ff0000'; \
               src.fillRect(0, 0, 1, 2); \
               src.fillStyle = '#0000ff'; \
               src.fillRect(1, 0, 1, 2); \
               const dst = canvases[1].getContext('2d'); \
               const p = dst.createPattern(canvases[0], 'repeat'); \
               dst.fillStyle = p; \
               dst.fillRect(0, 0, 2, 2); \
               const data = dst.getImageData(0, 0, 2, 2).data; \
               const leftIdx = (0 * 2 + 0) * 4; \
               const rightIdx = (0 * 2 + 1) * 4; \
               return `${data[leftIdx]},${data[leftIdx+2]},${data[rightIdx]},${data[rightIdx+2]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "255,0,0,255");
}

#[test]
fn create_pattern_returns_null_for_a_non_canvas_source() {
    let (rt, d) = context_with_two_canvases();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const dst = document.querySelectorAll('canvas')[1].getContext('2d'); \
               return dst.createPattern(document.body, 'repeat') === null; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}
