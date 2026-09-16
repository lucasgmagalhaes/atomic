//! `fillStyle`/`strokeStyle`/`lineWidth` and `fillRect`/`strokeRect`/
//! `getImageData` - `crate::canvas_bindings::fill_stroke`/`image_data`.
mod common;

use common::dom_with_canvas;
use js_runtime::{Context, Runtime};

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
