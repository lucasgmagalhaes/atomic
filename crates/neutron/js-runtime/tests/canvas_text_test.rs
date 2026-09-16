//! `ctx.font`/`fillText`/`strokeText`/`measureText` -
//! `crate::canvas_bindings::text`.
mod common;

use common::dom_with_canvas;
use js_runtime::{Context, Runtime};

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
