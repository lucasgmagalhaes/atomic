//! Real `HTMLCanvasElement.getContext('2d')` (`crate::canvas_bindings`) —
//! see that module's own doc for the exact scope cut (`fillRect`/
//! `clearRect`/`fillStyle` only, `#rrggbb`/`#rgb` hex only, a new JS
//! wrapper object per `getContext()` call sharing one real backing
//! `render::Canvas2D`).

use js_runtime::{Context, Runtime};

fn dom_with_canvas() -> (dom::Dom, dom::NodeId) {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.append_child(body, canvas);
    (d, canvas)
}

#[test]
fn get_context_2d_returns_a_real_context() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const c = document.querySelector('canvas'); \
               const ctx2d = c.getContext('2d'); \
               return `${ctx2d !== null},${typeof ctx2d.fillRect},${typeof ctx2d.clearRect}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,function,function");
}

#[test]
fn get_context_with_an_unsupported_id_returns_null() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const c = document.querySelector('canvas'); \
               return c.getContext('webgl') === null; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn fill_style_round_trips_a_hex_color() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               return ctx2d.fillStyle; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "#ff0000");
}

#[test]
fn a_3_digit_hex_fill_style_expands_correctly() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#0f0'; \
               return ctx2d.fillStyle; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "#00ff00");
}

#[test]
fn repeated_get_context_calls_share_one_real_backing() {
    // Real per-canvas persistence (`canvas_bindings`'s own doc): each
    // `getContext('2d')` call returns a new JS wrapper object, but every
    // wrapper for the same canvas shares the exact same `render::Canvas2D`
    // backing - so state set through one wrapper is visible through
    // another later `getContext()` call.
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const c = document.querySelector('canvas'); \
               const a = c.getContext('2d'); \
               a.fillStyle = '#123456'; \
               const b = c.getContext('2d'); \
               return `${a !== b},${b.fillStyle}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,#123456");
}

#[test]
fn an_unrecognized_context_id_never_creates_a_backing() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    ctx.eval(
        "document.querySelector('canvas').getContext('nonsense')",
        "<test>",
    )
    .unwrap();
    assert!(!ctx.has_active_canvases());
}

#[test]
fn getting_a_2d_context_registers_it_as_an_active_canvas() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    ctx.eval(
        "document.querySelector('canvas').getContext('2d')",
        "<test>",
    )
    .unwrap();
    assert!(ctx.has_active_canvases());
    assert_eq!(ctx.canvas_snapshots().len(), 1);
}

#[test]
fn get_image_data_returns_the_real_drawn_pixels() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "4");
    d.set_attribute(canvas, "height", "4");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const c = document.querySelector('canvas'); \
               const ctx2d = c.getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.fillRect(0, 0, 4, 4); \
               const img = ctx2d.getImageData(0, 0, 4, 4); \
               return `${img.width},${img.height},${img.data.length},${img.data[0]},${img.data[1]},${img.data[2]},${img.data[3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "4,4,64,255,0,0,255");
}

#[test]
fn stroke_style_and_line_width_round_trip() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.strokeStyle = '#123456'; \
               ctx2d.lineWidth = 3; \
               return `${ctx2d.strokeStyle},${ctx2d.lineWidth}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "#123456,3");
}

#[test]
fn stroke_rect_draws_a_visible_border_readable_via_get_image_data() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "4");
    d.set_attribute(canvas, "height", "4");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.strokeStyle = '#00ff00'; \
               ctx2d.lineWidth = 2; \
               ctx2d.strokeRect(1, 1, 2, 2); \
               const img = ctx2d.getImageData(0, 0, 4, 4); \
               return `${img.data[0]},${img.data[1]},${img.data[2]},${img.data[3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,255,0,255");
}

#[test]
fn put_image_data_writes_pixels_that_get_image_data_then_reads_back() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "4");
    d.set_attribute(canvas, "height", "4");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const c = document.querySelector('canvas'); \
               const ctx2d = c.getContext('2d'); \
               const data = new Uint8Array(4 * 4 * 4); \
               for (let i = 0; i < data.length; i += 4) { \
                 data[i] = 0; data[i + 1] = 255; data[i + 2] = 0; data[i + 3] = 255; \
               } \
               ctx2d.putImageData({ width: 4, height: 4, data }, 0, 0); \
               const img = ctx2d.getImageData(0, 0, 4, 4); \
               return `${img.data[0]},${img.data[1]},${img.data[2]},${img.data[3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,255,0,255");
}
