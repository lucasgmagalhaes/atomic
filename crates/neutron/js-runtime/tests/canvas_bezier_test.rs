//! `ctx.bezierCurveTo()`/`ctx.quadraticCurveTo()` -
//! `crate::canvas_bindings::path::{bezier_curve_to,quadratic_curve_to}`.
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
fn bezier_curve_to_paints_real_pixels_and_reaches_the_end_point() {
    let (rt, d) = context_with_canvas();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.beginPath(); \
               ctx2d.moveTo(5, 20); \
               ctx2d.bezierCurveTo(12, 2, 28, 2, 35, 20); \
               ctx2d.lineTo(35, 35); \
               ctx2d.lineTo(5, 35); \
               ctx2d.closePath(); \
               ctx2d.fill(); \
               const data = ctx2d.getImageData(0, 0, 40, 40).data; \
               const insideIdx = (30 * 40 + 20) * 4; \
               const cornerIdx = (1 * 40 + 1) * 4; \
               return `${data[insideIdx]},${data[insideIdx+1]},${data[insideIdx+2]},${data[insideIdx+3]},` + \
                      `${data[cornerIdx+3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "255,0,0,255,0");
}

#[test]
fn quadratic_curve_to_paints_real_pixels_and_reaches_the_end_point() {
    let (rt, d) = context_with_canvas();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#00ff00'; \
               ctx2d.beginPath(); \
               ctx2d.moveTo(5, 20); \
               ctx2d.quadraticCurveTo(20, 2, 35, 20); \
               ctx2d.lineTo(35, 35); \
               ctx2d.lineTo(5, 35); \
               ctx2d.closePath(); \
               ctx2d.fill(); \
               const data = ctx2d.getImageData(0, 0, 40, 40).data; \
               const insideIdx = (30 * 40 + 20) * 4; \
               return `${data[insideIdx]},${data[insideIdx+1]},${data[insideIdx+2]},${data[insideIdx+3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,255,0,255");
}

#[test]
fn bezier_curve_to_is_a_no_op_with_no_current_point() {
    let (rt, d) = context_with_canvas();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.beginPath(); \
               ctx2d.bezierCurveTo(10, 10, 20, 10, 30, 20); \
               ctx2d.fill(); \
               const data = ctx2d.getImageData(0, 0, 40, 40).data; \
               const idx = (20 * 40 + 20) * 4; \
               return data[idx + 3]; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0");
}
