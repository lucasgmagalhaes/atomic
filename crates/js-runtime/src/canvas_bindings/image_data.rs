//! `getImageData`/`putImageData` and `drawImage` - the raw-pixel surface
//! of `CanvasRenderingContext2D`.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::dom_bindings::node_id;

use super::context2d_opaque;
use super::helpers::{get_prop, read_js_f32, set_prop_f64};

/// `ctx.getImageData(x, y, w, h)` — scoped to always returning the
/// *whole* canvas's current pixels regardless of `x`/`y`/`w`/`h` (real
/// spec allows reading an arbitrary sub-rectangle; `render::Canvas2D::
/// get_image_data` only ever reads back the full backing texture, and
/// adding a windowed GPU readback wasn't worth it for this pass - a
/// documented, narrower-than-spec scope cut, same shape this crate's
/// other "one value, not the full generality" cuts already take). The
/// returned plain object is spec's own `ImageData` shape (`width`,
/// `height`, `data`) but not a real `ImageData` instance (`instanceof
/// ImageData` would be `false` - no such class is registered, since
/// nothing yet needs to construct one directly via `new ImageData(...)`).
/// `data` is a real `Uint8Array` (not spec's own `Uint8ClampedArray` -
/// this crate only has a `Uint8Array` constructor bound, same "one typed
/// array kind, not every element size" cut `crypto.getRandomValues`'s own
/// doc already takes) holding the exact tightly-packed RGBA8 bytes
/// `get_image_data` produced - reading/writing it as plain 0-255 byte
/// values already matches `Uint8ClampedArray`'s own clamping behavior
/// for any value actually written back through `putImageData`.
pub(super) unsafe extern "C" fn get_image_data(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_null();
    }
    let (width, height, pixels) = {
        let canvas = (*ptr).borrow();
        (canvas.width(), canvas.height(), canvas.get_image_data())
    };

    let obj = sys::JS_NewObject(ctx);
    set_prop_f64(ctx, obj, "width", width as f64);
    set_prop_f64(ctx, obj, "height", height as f64);
    let data = sys::JS_NewUint8ArrayCopy(ctx, pixels.as_ptr(), pixels.len());
    let data_name = CString::new("data").unwrap();
    sys::JS_SetPropertyStr(ctx, obj, data_name.as_ptr(), data);
    obj
}

/// `ctx.putImageData(imageData, x, y)` — writes `imageData.data` (read as
/// a real `Uint8Array`, per [`get_image_data`]'s own doc on the
/// `Uint8Array`-not-`Uint8ClampedArray` scope cut) directly into the
/// backing texture at `(x, y)`, clipped to the canvas's own bounds by
/// `Canvas2D::put_image_data`. No optional `dirtyX`/`dirtyY`/
/// `dirtyWidth`/`dirtyHeight` sub-rectangle arguments (real spec's own
/// 7-argument overload) - only the 3-argument form.
pub(super) unsafe extern "C" fn put_image_data(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() || argc < 3 {
        return sys::js_undefined();
    }
    let image_data = *argv;
    let x = read_js_f32(*argv.add(1)) as i32;
    let y = read_js_f32(*argv.add(2)) as i32;

    let width_val = get_prop(ctx, image_data, "width");
    let height_val = get_prop(ctx, image_data, "height");
    let w = read_js_f32(width_val) as u32;
    let h = read_js_f32(height_val) as u32;
    sys::JS_FreeValue(ctx, width_val);
    sys::JS_FreeValue(ctx, height_val);

    let data_val = get_prop(ctx, image_data, "data");
    let mut size: usize = 0;
    let data_ptr = sys::JS_GetUint8Array(ctx, &mut size, data_val);
    if !data_ptr.is_null() {
        let bytes = std::slice::from_raw_parts(data_ptr, size);
        (*ptr).borrow_mut().put_image_data(x, y, w, h, bytes);
    }
    sys::JS_FreeValue(ctx, data_val);
    sys::js_undefined()
}

/// `ctx.drawImage(source, dx, dy)` — scoped to just this 3-arg form
/// (real spec also has a `dw`/`dh` scaling overload and a 9-arg source-
/// rectangle overload, neither wired here) and to a `<canvas>` element as
/// `source` (real spec also accepts `HTMLImageElement`/`HTMLVideoElement`/
/// `ImageBitmap`/`OffscreenCanvas` - none of those are wired here yet).
/// The source canvas must already have an active 2D context (i.e.
/// `getContext('2d')` was called on it at least once, registering it in
/// `HostState::canvases` - see `super::element::get_context`'s own doc)
/// or this silently does nothing, matching this crate's general
/// "best-effort, no separate error path" convention. Implemented as a
/// full-canvas `get_image_data`/`put_image_data` round trip - no new
/// `render::Canvas2D` API needed, both primitives already exist.
pub(super) unsafe extern "C" fn draw_image(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() || argc < 3 {
        return sys::js_undefined();
    }
    let Some(source_node) = node_id(ctx, *argv) else {
        return sys::js_undefined();
    };
    let dx = read_js_f32(*argv.add(1)) as i32;
    let dy = read_js_f32(*argv.add(2)) as i32;

    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return sys::js_undefined();
    }
    let Some(source_backing) = (*state).canvases.get(&source_node).cloned() else {
        return sys::js_undefined();
    };
    let (w, h, pixels) = {
        let source = source_backing.borrow();
        (source.width(), source.height(), source.get_image_data())
    };
    (*ptr).borrow_mut().put_image_data(dx, dy, w, h, &pixels);
    sys::js_undefined()
}
