//! `drawImage` and `putImageData` - `crate::canvas_bindings::image_data`.
use js_runtime::{Context, Runtime};

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
