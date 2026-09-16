//! `save`/`restore`/`translate` - `crate::canvas_bindings::state`.
mod common;

use common::dom_with_canvas;
use js_runtime::{Context, Runtime};

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
