//! `ctx.scale()`/`ctx.rotate()`/`ctx.setTransform()`/`ctx.resetTransform()`
//! - `crate::canvas_bindings::state::{scale,rotate,set_transform,reset_transform}`.
use js_runtime::{Context, Runtime};

fn context_with_canvas() -> (Runtime, dom::Dom) {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "40");
    d.set_attribute(canvas, "height", "40");
    d.append_child(body, canvas);
    (Runtime::new(), d)
}

#[test]
fn scale_stretches_a_rect_real_pixels() {
    let (rt, d) = context_with_canvas();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.scale(2, 2); \
               ctx2d.fillRect(2, 2, 5, 5); \
               const data = ctx2d.getImageData(0, 0, 40, 40).data; \
               const insideIdx = (8 * 40 + 8) * 4; \
               const outsideIdx = (3 * 40 + 3) * 4; \
               return `${data[insideIdx+3]},${data[outsideIdx+3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "255,0");
}

#[test]
fn rotate_moves_a_rect_real_pixels() {
    let (rt, d) = context_with_canvas();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.translate(20, 20); \
               ctx2d.rotate(Math.PI / 2); \
               ctx2d.fillRect(2, -2, 8, 4); \
               const data = ctx2d.getImageData(0, 0, 40, 40).data; \
               const belowIdx = (25 * 40 + 20) * 4; \
               const rightIdx = (20 * 40 + 25) * 4; \
               return `${data[belowIdx+3]},${data[rightIdx+3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "255,0");
}

#[test]
fn set_transform_replaces_and_reset_transform_returns_to_identity() {
    let (rt, d) = context_with_canvas();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.translate(100, 100); \
               ctx2d.setTransform(1, 0, 0, 1, 0, 0); \
               ctx2d.scale(5, 5); \
               ctx2d.resetTransform(); \
               ctx2d.fillRect(5, 5, 10, 10); \
               const data = ctx2d.getImageData(0, 0, 40, 40).data; \
               const idx = (10 * 40 + 10) * 4; \
               return data[idx + 3]; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "255");
}
