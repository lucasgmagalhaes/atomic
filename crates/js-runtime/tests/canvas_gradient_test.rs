//! `createLinearGradient`/`createRadialGradient` and `fillStyle`'s
//! gradient-object branch - `crate::canvas_bindings::gradient`.
mod common;

use common::dom_with_canvas;
use js_runtime::{Context, Runtime};

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
