//! `HTMLCanvasElement`'s own methods - `getContext(id)`, `toDataURL()`,
//! `toBlob()`. Unlike everything else in `canvas_bindings`, these live on
//! the canvas element itself, not on `CanvasRenderingContext2D`.
use std::cell::RefCell;
use std::os::raw::{c_int, c_void};
use std::rc::Rc;

use quickjs_sys as sys;

use render::Canvas2D;

use crate::dom_bindings::node_id;
use crate::js_helpers::define_method;

use super::helpers::read_js_string;
use super::CONTEXT_2D_CLASS_KIND;

const DEFAULT_CANVAS_WIDTH: u32 = 300;
const DEFAULT_CANVAS_HEIGHT: u32 = 150;

pub(super) unsafe extern "C" fn get_context(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_null();
    }
    let Some(context_id) = read_js_string(ctx, *argv) else {
        return sys::js_null();
    };
    if context_id != "2d" {
        return sys::js_null();
    }
    let Some(node) = node_id(ctx, this_val) else {
        return sys::js_null();
    };

    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return sys::js_null();
    }
    let backing = if let Some(existing) = (*state).canvases.get(&node) {
        existing.clone()
    } else {
        let width = (*state)
            .dom
            .attribute(node, "width")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(DEFAULT_CANVAS_WIDTH);
        let height = (*state)
            .dom
            .attribute(node, "height")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(DEFAULT_CANVAS_HEIGHT);
        let backing = Rc::new(RefCell::new(Canvas2D::new(width, height)));
        (*state).canvases.insert(node, backing.clone());
        backing
    };

    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::class_id_for(rt, CONTEXT_2D_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    sys::JS_SetOpaque(obj, Box::into_raw(Box::new(backing)) as *mut c_void);
    obj
}

/// `canvas.toDataURL()` — a real spec method on `HTMLCanvasElement`
/// itself (not `CanvasRenderingContext2D`). Requires `getContext('2d')` to
/// have already been called on this element at least once (i.e. it's
/// registered in `HostState::canvases`) - a canvas nothing has ever drawn
/// through returns `""` rather than a data URL for an all-transparent
/// image, a documented gap (real spec would still produce one). Ignores
/// any `type`/`quality` arguments - see `render::Canvas2D::to_data_url`'s
/// own doc for why the output is always PNG.
pub(super) unsafe extern "C" fn to_data_url(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(node) = node_id(ctx, this_val) else {
        return sys::JS_NewStringLen(ctx, "".as_ptr() as *const std::os::raw::c_char, 0);
    };
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return sys::JS_NewStringLen(ctx, "".as_ptr() as *const std::os::raw::c_char, 0);
    }
    let Some(backing) = (*state).canvases.get(&node) else {
        return sys::JS_NewStringLen(ctx, "".as_ptr() as *const std::os::raw::c_char, 0);
    };
    let url = backing.borrow().to_data_url();
    sys::JS_NewStringLen(ctx, url.as_ptr() as *const std::os::raw::c_char, url.len())
}

/// Calls `callback(arg)` synchronously and frees both the call result and
/// `arg` - the shared tail of [`to_blob`]'s every branch.
unsafe fn call_with_arg(ctx: *mut sys::JSContext, callback: sys::JSValue, mut arg: sys::JSValue) {
    let result = sys::JS_Call(ctx, callback, sys::js_undefined(), 1, &mut arg);
    sys::JS_FreeValue(ctx, result);
    sys::JS_FreeValue(ctx, arg);
}

/// `canvas.toBlob(callback, type, quality)` — a real spec method on
/// `HTMLCanvasElement` itself. Real spec calls `callback` asynchronously
/// (a task queued on the event loop); this crate has no task queue for
/// that, so `callback` runs **synchronously** instead, right before
/// `toBlob` itself returns - a documented timing deviation, not just a
/// missing feature (a caller relying on `toBlob` returning before its
/// callback fires would observe different ordering here). The `Blob` it
/// receives is real (`crate::blob`'s own class - real byte storage,
/// `size`/`type`/`slice`/`text`/`arrayBuffer`), wrapping real PNG bytes
/// from `render::Canvas2D::to_png_bytes` (same bytes `toDataURL` itself
/// base64-encodes - see that function's own doc for the "always PNG"
/// scope cut, which applies here too; `type`/`quality` arguments are
/// accepted but ignored). `callback(null)` when the canvas has no active
/// 2D context, matching `toDataURL`'s own `""` case.
pub(super) unsafe extern "C" fn to_blob(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let callback = *argv;
    let backing = node_id(ctx, this_val).and_then(|node| {
        let state = crate::host_state::get(ctx);
        if state.is_null() {
            None
        } else {
            (*state).canvases.get(&node).cloned()
        }
    });
    let Some(backing) = backing else {
        call_with_arg(ctx, callback, sys::js_null());
        return sys::js_undefined();
    };

    let bytes = backing.borrow().to_png_bytes();
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::class_id_for(rt, crate::blob::BLOB_CLASS_KIND);
    let blob_obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&blob_obj) {
        call_with_arg(ctx, callback, sys::js_null());
        return sys::js_undefined();
    }
    let inner = crate::blob::BlobInner {
        bytes,
        mime: "image/png".to_string(),
    };
    sys::JS_SetOpaque(blob_obj, Box::into_raw(Box::new(inner)) as *mut c_void);
    call_with_arg(ctx, callback, blob_obj);
    sys::js_undefined()
}

/// Adds `getContext(id)`/`toDataURL()`/`toBlob()` to
/// `HTMLCanvasElement.prototype` - called from
/// `element_classes::classes::ensure_html_subclass`'s own
/// `HTML_CANVAS_CLASS_KIND` branch, same wiring point every other
/// element-specific method set (`define_form_properties`,
/// `define_select_properties`, ...) already uses.
pub(crate) unsafe fn define_canvas_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    define_method(ctx, proto, "getContext", get_context, 1);
    define_method(ctx, proto, "toDataURL", to_data_url, 0);
    define_method(ctx, proto, "toBlob", to_blob, 1);
}
