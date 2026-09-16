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
fn save_and_restore_round_trip_fill_style() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.save(); \
               ctx2d.fillStyle = '#00ff00'; \
               ctx2d.restore(); \
               return ctx2d.fillStyle; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "#ff0000");
}

#[test]
fn restore_with_nothing_saved_does_not_throw() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.restore(); \
               return 'ok'; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "ok");
}

#[test]
fn translate_offsets_a_subsequent_fill_rect() {
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
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.translate(2, 2); \
               ctx2d.fillRect(0, 0, 1, 1); \
               const img = ctx2d.getImageData(0, 0, 4, 4); \
               const idx = (2 * 4 + 2) * 4; \
               return `${img.data[idx]},${img.data[idx + 1]},${img.data[idx + 2]},${img.data[idx + 3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "255,0,0,255");
}

#[test]
fn linear_gradient_fill_paints_a_visible_blend_across_a_wide_rect() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "200");
    d.set_attribute(canvas, "height", "1");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               const g = ctx2d.createLinearGradient(0, 0, 200, 0); \
               g.addColorStop(0, '#ff0000'); \
               g.addColorStop(1, '#0000ff'); \
               ctx2d.fillStyle = g; \
               ctx2d.fillRect(0, 0, 200, 1); \
               const img = ctx2d.getImageData(0, 0, 200, 1); \
               const startIdx = 0; \
               const endIdx = 199 * 4; \
               return `${img.data[startIdx] > 250},${img.data[startIdx + 2] < 10},` + \
                      `${img.data[endIdx + 2] > 250},${img.data[endIdx] < 10}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true,true");
}

#[test]
fn setting_fill_style_to_a_string_after_a_gradient_reverts_to_solid() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               const g = ctx2d.createLinearGradient(0, 0, 10, 0); \
               g.addColorStop(0, '#ff0000'); \
               g.addColorStop(1, '#0000ff'); \
               ctx2d.fillStyle = g; \
               ctx2d.fillStyle = '#00ff00'; \
               return ctx2d.fillStyle; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "#00ff00");
}

#[test]
fn draw_image_copies_pixels_from_a_source_canvas_to_the_destination() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let source = d.create_element("canvas");
    d.set_attribute(source, "width", "2");
    d.set_attribute(source, "height", "2");
    d.append_child(body, source);
    let dest = d.create_element("canvas");
    d.set_attribute(dest, "width", "4");
    d.set_attribute(dest, "height", "4");
    d.append_child(body, dest);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const canvases = document.querySelectorAll('canvas'); \
               const src = canvases[0]; \
               const dst = canvases[1]; \
               const srcCtx = src.getContext('2d'); \
               srcCtx.fillStyle = '#ff00ff'; \
               srcCtx.fillRect(0, 0, 2, 2); \
               const dstCtx = dst.getContext('2d'); \
               dstCtx.drawImage(src, 1, 1); \
               const img = dstCtx.getImageData(0, 0, 4, 4); \
               const idx = (1 * 4 + 1) * 4; \
               return `${img.data[idx]},${img.data[idx + 1]},${img.data[idx + 2]},${img.data[idx + 3]}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "255,0,255,255");
}

#[test]
fn draw_image_with_a_source_that_has_no_2d_context_is_a_no_op() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let source = d.create_element("canvas");
    d.append_child(body, source);
    let dest = d.create_element("canvas");
    d.append_child(body, dest);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const canvases = document.querySelectorAll('canvas'); \
               const dstCtx = canvases[1].getContext('2d'); \
               dstCtx.drawImage(canvases[0], 0, 0); \
               return 'ok'; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "ok");
}

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

#[test]
fn fill_text_paints_visible_pixels() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "60");
    d.set_attribute(canvas, "height", "30");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.fillText('Hi', 5, 5); \
               const data = ctx2d.getImageData(0, 0, 60, 30).data; \
               let painted = false; \
               for (let i = 3; i < data.length; i += 4) { \
                 if (data[i] !== 0) { painted = true; break; } \
               } \
               return painted; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn fill_text_with_empty_string_paints_nothing() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.fillText('', 5, 5); \
               const data = ctx2d.getImageData(0, 0, 300, 150).data; \
               let painted = false; \
               for (let i = 3; i < data.length; i += 4) { \
                 if (data[i] !== 0) { painted = true; break; } \
               } \
               return painted; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false");
}

#[test]
fn to_data_url_returns_a_real_png_data_url_after_drawing() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const c = document.querySelector('canvas'); \
               const ctx2d = c.getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.fillRect(0, 0, 10, 10); \
               return c.toDataURL().startsWith('data:image/png;base64,'); \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn to_data_url_with_no_active_context_returns_empty_string() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval("document.querySelector('canvas').toDataURL()", "<test>")
        .unwrap();
    assert_eq!(result, "");
}

#[test]
fn font_getter_setter_round_trip() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.font = '24px monospace'; \
               return ctx2d.font; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "24px monospace");
}

#[test]
fn measure_text_returns_an_object_with_a_positive_width() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               return ctx2d.measureText('Hello').width > 0; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn stroke_text_paints_strokestyle_not_fillstyle() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "60");
    d.set_attribute(canvas, "height", "30");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               ctx2d.fillStyle = '#ff0000'; \
               ctx2d.strokeStyle = '#0000ff'; \
               ctx2d.strokeText('Hi', 5, 5); \
               const data = ctx2d.getImageData(0, 0, 60, 30).data; \
               let anyRed = false, anyBlue = false; \
               for (let i = 0; i < data.length; i += 4) { \
                 if (data[i] === 255 && data[i+2] === 0 && data[i+3] !== 0) anyRed = true; \
                 if (data[i+2] === 255 && data[i] === 0 && data[i+3] !== 0) anyBlue = true; \
               } \
               return `${anyRed},${anyBlue}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false,true");
}

#[test]
fn radial_gradient_fill_paints_a_real_circular_blend() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    d.append_child(root, body);
    let canvas = d.create_element("canvas");
    d.set_attribute(canvas, "width", "400");
    d.set_attribute(canvas, "height", "400");
    d.append_child(body, canvas);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const ctx2d = document.querySelector('canvas').getContext('2d'); \
               const g = ctx2d.createRadialGradient(200, 200, 0, 200, 200, 200); \
               g.addColorStop(0, '#ff0000'); \
               g.addColorStop(1, '#0000ff'); \
               ctx2d.fillStyle = g; \
               ctx2d.fillRect(0, 0, 400, 400); \
               const img = ctx2d.getImageData(0, 0, 400, 400); \
               const centerIdx = (200 * 400 + 200) * 4; \
               const cornerIdx = 0; \
               return `${img.data[centerIdx] > 250},${img.data[centerIdx + 2] < 15},` + \
                      `${img.data[cornerIdx + 2] > 200},${img.data[cornerIdx] < 30}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true,true");
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
