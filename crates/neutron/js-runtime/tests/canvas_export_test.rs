//! `toDataURL`/`toBlob` - `crate::canvas_bindings::element`.
mod common;

use common::dom_with_canvas;
use js_runtime::{Context, Runtime};

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
fn to_blob_calls_back_with_a_real_blob() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               const c = document.querySelector('canvas'); \
               const ctx2d = c.getContext('2d'); \
               ctx2d.fillStyle = '#00ff00'; \
               ctx2d.fillRect(0, 0, 10, 10); \
               let captured = null; \
               c.toBlob((blob) => { captured = blob; }); \
               return `${captured !== null},${captured.type},${captured.size > 0}`; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,image/png,true");
}

#[test]
fn to_blob_with_no_active_context_calls_back_with_null() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
               let captured = 'not called'; \
               document.querySelector('canvas').toBlob((blob) => { captured = blob; }); \
               return captured === null; \
             })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn to_blob_produces_a_real_decodable_png() {
    let (d, _) = dom_with_canvas();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);
    ctx.eval(
        "(() => { \
           const c = document.querySelector('canvas'); \
           const ctx2d = c.getContext('2d'); \
           ctx2d.fillStyle = '#0000ff'; \
           ctx2d.fillRect(0, 0, 4, 4); \
           globalThis.__bytesLen = null; \
           c.toBlob((blob) => { \
             blob.arrayBuffer().then((buf) => { globalThis.__bytesLen = buf.byteLength; }); \
           }); \
         })()",
        "<test>",
    )
    .unwrap();
    ctx.run_pending_timers();
    let result = ctx.eval("globalThis.__bytesLen > 0", "<test>").unwrap();
    assert_eq!(result, "true");
}
