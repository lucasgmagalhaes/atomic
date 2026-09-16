//! `HTMLCanvasElement.getContext('2d')` itself - `crate::canvas_bindings`'s
//! `element::get_context`.
mod common;

use common::dom_with_canvas;
use js_runtime::{Context, Runtime};

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
