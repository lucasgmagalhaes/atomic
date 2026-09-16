//! `ctx.arc()` - `crate::canvas_bindings::path::arc`.
use js_runtime::{Context, Runtime};

#[test]
fn arc_fills_a_full_circle() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "40");
    d.set_attribute(canvas, "height", "40");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#00ff00'; \
               ctx2d.beginPath(); \
               ctx2d.arc(20, 20, 15, 0, Math.PI * 2, false); \
               ctx2d.fill(); \
               const data = ctx2d.getImageData(0, 0, 40, 40).data; \
               const centerIdx = (20 * 40 + 20) * 4; \
               const cornerIdx = (1 * 40 + 1) * 4; \
               return `${data[centerIdx]},${data[centerIdx+1]},${data[centerIdx+2]},${data[centerIdx+3]},` + \
                      `${data[cornerIdx+3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,255,0,255,0");
}

#[test]
fn arc_defaults_anticlockwise_to_false_when_omitted() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "40");
    d.set_attribute(canvas, "height", "40");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.beginPath(); \
               ctx2d.arc(20, 20, 15, 0, Math.PI * 2); \
               ctx2d.fill(); \
               const data = ctx2d.getImageData(0, 0, 40, 40).data; \
               const centerIdx = (20 * 40 + 20) * 4; \
               return data[centerIdx + 3] !== 0; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}
