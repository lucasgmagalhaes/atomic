//! `beginPath`/`moveTo`/`lineTo`/`closePath`/`fill` -
//! `crate::canvas_bindings::path`.
use js_runtime::{Context, Runtime};

#[test]
fn fill_paints_a_triangle_from_a_path() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "10");
    d.set_attribute(canvas, "height", "10");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#00ff00'; \
               ctx2d.beginPath(); \
               ctx2d.moveTo(0, 0); \
               ctx2d.lineTo(10, 0); \
               ctx2d.lineTo(0, 10); \
               ctx2d.closePath(); \
               ctx2d.fill(); \
               const inside = ctx2d.getImageData(0, 0, 10, 10).data; \
               const insideIdx = (1 * 10 + 1) * 4; \
               const outsideIdx = (9 * 10 + 9) * 4; \
               return `${inside[insideIdx]},${inside[insideIdx+1]},${inside[insideIdx+2]},${inside[insideIdx+3]},` + \
                      `${inside[outsideIdx]},${inside[outsideIdx+3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,255,0,255,0,0");
}

#[test]
fn fill_with_fewer_than_3_points_does_nothing() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "10");
    d.set_attribute(canvas, "height", "10");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.beginPath(); \
               ctx2d.moveTo(0, 0); \
               ctx2d.lineTo(10, 10); \
               ctx2d.fill(); \
               const idx = (1 * 10 + 1) * 4; \
               const data = ctx2d.getImageData(0, 0, 10, 10).data; \
               return `${data[idx]},${data[idx + 3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,0");
}
