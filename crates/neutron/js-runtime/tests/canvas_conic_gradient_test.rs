//! `ctx.createConicGradient()` - `crate::canvas_bindings::gradient::create_conic_gradient`.
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
fn conic_gradient_paints_real_blended_pixels_via_js() {
    let (rt, d) = context_with_canvas();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               const g = ctx2d.createConicGradient(0, 20, 20); \
               g.addColorStop(0, '#ff0000'); \
               g.addColorStop(1, '#0000ff'); \
               ctx2d.fillStyle = g; \
               ctx2d.fillRect(0, 0, 40, 40); \
               const data = ctx2d.getImageData(0, 0, 40, 40).data; \
               const startIdx = (20 * 40 + 38) * 4; \
               return `${data[startIdx] > 200},${data[startIdx+2] < 60},${data[startIdx+3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,255");
}
