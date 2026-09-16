//! Real `HTMLCanvasElement.getContext('2d')` (`spec/matrix/browser-apis.md`'s
//! Canvas2D gap): wires the already-existing, GPU-backed `render::Canvas2D`
//! (`fillRect`/`clearRect`/`fillStyle` — see that module's own scope-cut
//! doc, which this binding inherits unchanged) into JavaScript and into
//! this worker's real page compositing.
//!
//! `getContext(id)` only recognizes `"2d"` (any other value, including
//! `"webgl"`, returns `null` - no WebGL/`OffscreenCanvas` context here).
//! The returned `CanvasRenderingContext2D` is a real, minimal object with
//! just the three methods/properties `render::Canvas2D` itself
//! implements - no paths, strokes, text, images, gradients, or
//! transforms. Real per-canvas persistence: the backing `render::Canvas2D`
//! lives in `HostState::canvases`, keyed by the canvas element's own
//! `NodeId`, so its drawn pixels and `fillStyle` survive across separate
//! `getContext('2d')` calls on the same element - the one real deviation
//! from spec is that each `getContext()` call returns a **new JS wrapper
//! object**, not the literal same cached one a real browser returns
//! (`===` across two calls would be `false` here) - this crate has no
//! per-`(ctx, NodeId)` `JSValue` cache for this yet (the pattern
//! `class_list`/`dataset`/`css_style` already use for other per-element
//! objects would be the natural next step, left for whenever a real page
//! is found relying on getContext's object identity, not just its drawn
//! state). Real `width`/`height` HTML attributes (defaulting to the spec's
//! own `300`/`150`) size the canvas's backing pixel buffer, read once at
//! the first `getContext('2d')` call - not on every call, and not
//! re-read on a later attribute mutation (this crate has no notion of
//! resizing a live `wgpu::Texture` in place).
//!
//! Real page compositing (`profile-worker`'s `Page::render`, via
//! `Context::canvas_snapshots`): every active canvas's current pixels
//! (`Canvas2D::get_image_data`) are re-read fresh every real paint and fed
//! into `layout_engine::apply_canvas_snapshots`, which reuses the
//! *existing*, already-tested `<img>` compositing pipeline
//! (`render::build_image_list`/`composite_images`) - a canvas is painted
//! exactly like any other replaced element, no new paint primitive. Since
//! this engine has no cheap way to detect whether a canvas's drawn
//! content actually changed since the last frame (no dirty-tracking on
//! `fillRect`/`clearRect`), a page with any active 2D canvas context
//! simply bypasses `PaintCache`/per-layer caching on *every* frame while
//! that canvas exists - see `Page::render`'s own doc at its
//! `has_active_canvases` check.

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};
use std::rc::Rc;

use quickjs_sys as sys;

use layout_engine::Color;
use render::Canvas2D;

use crate::dom_bindings::node_id;
use crate::js_helpers::{define_getter_setter, define_method, Getter, Setter};

const CONTEXT_2D_CLASS_KIND: &str = "CanvasRenderingContext2D";

const DEFAULT_CANVAS_WIDTH: u32 = 300;
const DEFAULT_CANVAS_HEIGHT: u32 = 150;

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
    let mut len: usize = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, val, false);
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
    let s = String::from_utf8_lossy(bytes).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

unsafe fn read_js_f32(val: sys::JSValue) -> f32 {
    match val.tag {
        sys::JS_TAG_INT => val.u.int32 as f32,
        sys::JS_TAG_FLOAT64 => val.u.float64 as f32,
        _ => 0.0,
    }
}

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str) -> sys::JSValue {
    let name = CString::new(key).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

unsafe fn set_prop_f64(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: f64) {
    let name = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), sys::js_float64(val));
}

/// `#rgb`/`#rrggbb` only - no `rgb()`/`rgba()`/`hsl()`/named color
/// keywords. Neither `render::Canvas2D` nor a publicly exposed
/// `layout_engine` API has a general CSS-color-string parser to reuse
/// (the one `layout-engine` has, `Color::named`/`from_hex`, is
/// `pub(super)`-private to its own style-cascade module) - this is a
/// small, self-contained parser scoped to the one real-world-common
/// pattern (`ctx.fillStyle = "#ff0000"`), matching this crate's own
/// "narrower than spec, clearly documented" convention.
fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    let (r, g, b) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ),
        3 => {
            let double = |c: char| u8::from_str_radix(&format!("{c}{c}"), 16).ok();
            let mut chars = hex.chars();
            (
                double(chars.next()?)?,
                double(chars.next()?)?,
                double(chars.next()?)?,
            )
        }
        _ => return None,
    };
    Some(Color { r, g, b, a: 255 })
}

fn format_hex_color(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

unsafe fn context2d_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut Rc<RefCell<Canvas2D>> {
    sys::JS_GetOpaque(
        this_val,
        crate::class_registry::class_id_for(rt, CONTEXT_2D_CLASS_KIND),
    ) as *mut Rc<RefCell<Canvas2D>>
}

unsafe extern "C" fn context2d_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = context2d_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

unsafe extern "C" fn fill_rect(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 4 {
        let (x, y, w, h) = (
            read_js_f32(*argv),
            read_js_f32(*argv.add(1)),
            read_js_f32(*argv.add(2)),
            read_js_f32(*argv.add(3)),
        );
        (*ptr).borrow_mut().fill_rect(x, y, w, h);
    }
    sys::js_undefined()
}

unsafe extern "C" fn clear_rect(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 4 {
        let (x, y, w, h) = (
            read_js_f32(*argv),
            read_js_f32(*argv.add(1)),
            read_js_f32(*argv.add(2)),
            read_js_f32(*argv.add(3)),
        );
        (*ptr).borrow_mut().clear_rect(x, y, w, h);
    }
    sys::js_undefined()
}

unsafe extern "C" fn fill_style_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    let color = if ptr.is_null() {
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    } else {
        (*ptr).borrow().fill_style()
    };
    let s = format_hex_color(color);
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe extern "C" fn fill_style_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            if let Some(color) = parse_hex_color(s.trim()) {
                (*ptr).borrow_mut().set_fill_style(color);
            }
        }
    }
    sys::js_undefined()
}

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
unsafe extern "C" fn get_image_data(
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
unsafe extern "C" fn put_image_data(
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

/// Registers the `CanvasRenderingContext2D` class on `ctx`'s runtime (if
/// not already done) - no global constructor is exposed, matching real
/// spec (it's never `new`-able; only reachable via `getContext('2d')`).
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new(CONTEXT_2D_CLASS_KIND).unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(context2d_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, CONTEXT_2D_CLASS_KIND, &def);
    let existing = sys::JS_GetClassProto(ctx, class_id);
    let already_registered = existing.tag == sys::JS_TAG_OBJECT;
    sys::JS_FreeValue(ctx, existing);
    if already_registered {
        return;
    }

    let proto = sys::JS_NewObject(ctx);
    define_method(ctx, proto, "fillRect", fill_rect, 4);
    define_method(ctx, proto, "clearRect", clear_rect, 4);
    define_method(ctx, proto, "getImageData", get_image_data, 4);
    define_method(ctx, proto, "putImageData", put_image_data, 3);
    define_getter_setter(
        ctx,
        proto,
        "fillStyle",
        fill_style_get as Getter,
        fill_style_set as Setter,
    );
    sys::JS_SetClassProto(ctx, class_id, proto);
}

unsafe extern "C" fn get_context(
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

/// Adds `getContext(id)` to `HTMLCanvasElement.prototype` - called from
/// `element_classes::classes::ensure_html_subclass`'s own
/// `HTML_CANVAS_CLASS_KIND` branch, same wiring point every other
/// element-specific method set (`define_form_properties`,
/// `define_select_properties`, ...) already uses.
pub(crate) unsafe fn define_canvas_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    define_method(ctx, proto, "getContext", get_context, 1);
}
