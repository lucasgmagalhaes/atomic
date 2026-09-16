//! Real `HTMLCanvasElement.getContext('2d')` (`spec/matrix/browser-apis.md`'s
//! Canvas2D gap): wires the already-existing, GPU-backed `render::Canvas2D`
//! (`fillRect`/`clearRect`/`fillStyle`/`strokeRect`/`strokeStyle`/
//! `lineWidth`/`save`/`restore`/`translate`/`createLinearGradient`/
//! `createRadialGradient` — see that module's own scope-cut doc, which
//! this binding inherits unchanged) into JavaScript and into this
//! worker's real page
//! compositing. Also adds `drawImage(source, dx, dy)` - canvas-to-canvas
//! only (see [`draw_image`]'s own doc), built entirely from
//! `get_image_data`/`put_image_data` with no new `render::Canvas2D` API -
//! and `beginPath`/`moveTo`/`lineTo`/`closePath`/`fill` (convex polygons
//! only, no stroke, no curves - see `render::Canvas2D::fill`'s own doc) -
//! plus real `ctx.font`, `fillText`/`strokeText`/`measureText` (no
//! `maxWidth` wrapping, `strokeText` isn't a real outline stroke - see
//! `render::Canvas2D::fill_text`/`stroke_text`/`set_font`'s own docs).
//! Also adds `toDataURL()`/`toBlob()` on `HTMLCanvasElement` itself (see
//! [`to_data_url`]/[`to_blob`]'s own docs - real PNG encoding, requires
//! an already-active 2D context; `toBlob`'s callback runs synchronously,
//! not queued as a real spec-shaped async task).
//!
//! `getContext(id)` only recognizes `"2d"` (any other value, including
//! `"webgl"`, returns `null` - no WebGL/`OffscreenCanvas` context here).
//! The returned `CanvasRenderingContext2D` is a real, minimal object with
//! just the methods/properties `render::Canvas2D` itself implements - no
//! paths, general strokes, text, `<img>`/`<video>`/`ImageBitmap` image
//! sources, conic gradients or patterns, or `scale`/`rotate`/general
//! transform matrix (see that module's own doc for exactly what *is*
//! real, including 2-stop `createLinearGradient`/`createRadialGradient`).
//! Real per-canvas persistence: the backing `render::Canvas2D`
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
use render::{Canvas2D, FillGradient, LinearGradient, RadialGradient};

use crate::dom_bindings::node_id;
use crate::js_helpers::{define_getter_setter, define_method, Getter, Setter};

const CONTEXT_2D_CLASS_KIND: &str = "CanvasRenderingContext2D";
const GRADIENT_CLASS_KIND: &str = "CanvasGradient";

const DEFAULT_CANVAS_WIDTH: u32 = 300;
const DEFAULT_CANVAS_HEIGHT: u32 = 150;

/// `ctx.createLinearGradient(...)`/`ctx.createRadialGradient(...)`'s
/// shared backing - one `CanvasGradient` class covers both (real spec's
/// own shape too: both factories return the same `CanvasGradient` type).
/// `addColorStop` just accumulates `(offset, color)` pairs regardless of
/// `kind`; [`resolve_gradient`] (called when this object is actually
/// assigned to `ctx.fillStyle`) is what reduces them down to the 2-stop
/// shape `render::Canvas2D` itself supports (see [`LinearGradient`]/
/// [`RadialGradient`]'s own docs on why only 2 effective stops).
enum GradientKind {
    Linear { x0: f32, y0: f32, x1: f32, y1: f32 },
    Radial { cx: f32, cy: f32, radius: f32 },
}

struct GradientData {
    kind: GradientKind,
    stops: Vec<(f32, Color)>,
}

/// Reduces an arbitrary-length stop list down to the start/end pair
/// `render::LinearGradient`/`RadialGradient` need: the color at the
/// smallest offset becomes `start`, the color at the largest becomes
/// `end`. A single stop is used for both ends (a solid-color "gradient");
/// zero stops resolves to nothing (real spec would render fully
/// transparent black - a call site that skips setting a fill style in
/// that case is an accepted, documented gap rather than matching that
/// exactly).
fn resolve_gradient(data: &GradientData) -> Option<FillGradient> {
    let mut min = None;
    let mut max = None;
    for &(offset, color) in &data.stops {
        if min.map(|(o, _)| offset < o).unwrap_or(true) {
            min = Some((offset, color));
        }
        if max.map(|(o, _)| offset >= o).unwrap_or(true) {
            max = Some((offset, color));
        }
    }
    let (_, start) = min?;
    let (_, end) = max?;
    Some(match data.kind {
        GradientKind::Linear { x0, y0, x1, y1 } => FillGradient::Linear(LinearGradient {
            x0,
            y0,
            x1,
            y1,
            start,
            end,
        }),
        GradientKind::Radial { cx, cy, radius } => FillGradient::Radial(RadialGradient {
            cx,
            cy,
            radius,
            start,
            end,
        }),
    })
}

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

/// `ctx.fillText(text, x, y)` — the `maxWidth` 4th argument isn't
/// accepted (see `render::Canvas2D::fill_text`'s own doc for this and its
/// other scope cuts).
unsafe extern "C" fn fill_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 3 {
        if let Some(text) = read_js_string(ctx, *argv) {
            let x = read_js_f32(*argv.add(1));
            let y = read_js_f32(*argv.add(2));
            (*ptr).borrow_mut().fill_text(&text, x, y);
        }
    }
    sys::js_undefined()
}

/// `ctx.strokeText(text, x, y)` — see `render::Canvas2D::stroke_text`'s
/// own doc: not a real outline stroke, paints the same glyphs solid in
/// `strokeStyle`.
unsafe extern "C" fn stroke_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 3 {
        if let Some(text) = read_js_string(ctx, *argv) {
            let x = read_js_f32(*argv.add(1));
            let y = read_js_f32(*argv.add(2));
            (*ptr).borrow_mut().stroke_text(&text, x, y);
        }
    }
    sys::js_undefined()
}

/// `ctx.measureText(text)` — returns a plain object with just `width`
/// (see `render::Canvas2D::measure_text`'s own doc for why no other
/// `TextMetrics` fields exist).
unsafe extern "C" fn measure_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    let width = if ptr.is_null() || argc < 1 {
        0.0
    } else {
        match read_js_string(ctx, *argv) {
            Some(text) => (*ptr).borrow().measure_text(&text),
            None => 0.0,
        }
    };
    let obj = sys::JS_NewObject(ctx);
    set_prop_f64(ctx, obj, "width", width as f64);
    obj
}

unsafe extern "C" fn font_get(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    let s = if ptr.is_null() {
        "16px sans-serif".to_string()
    } else {
        (*ptr).borrow().font()
    };
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe extern "C" fn font_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            (*ptr).borrow_mut().set_font(&s);
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn begin_path(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().begin_path();
    }
    sys::js_undefined()
}

unsafe extern "C" fn move_to(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 2 {
        let (x, y) = (read_js_f32(*argv), read_js_f32(*argv.add(1)));
        (*ptr).borrow_mut().move_to(x, y);
    }
    sys::js_undefined()
}

unsafe extern "C" fn line_to(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 2 {
        let (x, y) = (read_js_f32(*argv), read_js_f32(*argv.add(1)));
        (*ptr).borrow_mut().line_to(x, y);
    }
    sys::js_undefined()
}

unsafe extern "C" fn close_path(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().close_path();
    }
    sys::js_undefined()
}

unsafe extern "C" fn fill(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().fill();
    }
    sys::js_undefined()
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

unsafe extern "C" fn save(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().save();
    }
    sys::js_undefined()
}

unsafe extern "C" fn restore(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().restore();
    }
    sys::js_undefined()
}

unsafe extern "C" fn translate(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 2 {
        let (x, y) = (read_js_f32(*argv), read_js_f32(*argv.add(1)));
        (*ptr).borrow_mut().translate(x, y);
    }
    sys::js_undefined()
}

unsafe extern "C" fn stroke_rect(
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
        (*ptr).borrow_mut().stroke_rect(x, y, w, h);
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
    if ptr.is_null() {
        return sys::js_undefined();
    }
    // A `CanvasGradient` (from `createLinearGradient`) takes priority over
    // treating `val` as a hex string - real spec assigns whatever value's
    // own type dictates (string vs `CanvasGradient` object), this crate
    // just checks the one gradient class it actually has.
    if val.tag == sys::JS_TAG_OBJECT {
        let rt = sys::JS_GetRuntime(ctx);
        let class_id = crate::class_registry::class_id_for(rt, GRADIENT_CLASS_KIND);
        let gptr = sys::JS_GetOpaque(val, class_id) as *mut RefCell<GradientData>;
        if !gptr.is_null() {
            if let Some(gradient) = resolve_gradient(&(*gptr).borrow()) {
                (*ptr).borrow_mut().set_fill_gradient(gradient);
            }
            return sys::js_undefined();
        }
    }
    if let Some(s) = read_js_string(ctx, val) {
        if let Some(color) = parse_hex_color(s.trim()) {
            (*ptr).borrow_mut().set_fill_style(color);
        }
    }
    sys::js_undefined()
}

unsafe fn gradient_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut RefCell<GradientData> {
    sys::JS_GetOpaque(
        this_val,
        crate::class_registry::class_id_for(rt, GRADIENT_CLASS_KIND),
    ) as *mut RefCell<GradientData>
}

unsafe extern "C" fn gradient_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = gradient_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

/// `gradient.addColorStop(offset, color)` - `color` only accepts the same
/// `#rgb`/`#rrggbb` hex forms [`parse_hex_color`] does elsewhere.
unsafe extern "C" fn add_color_stop(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = gradient_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 2 {
        let offset = read_js_f32(*argv);
        if let Some(s) = read_js_string(ctx, *argv.add(1)) {
            if let Some(color) = parse_hex_color(s.trim()) {
                (*ptr).borrow_mut().stops.push((offset, color));
            }
        }
    }
    sys::js_undefined()
}

/// Registers the `CanvasGradient` class (if not already done) - like
/// `CanvasRenderingContext2D` itself, never `new`-able; only reachable via
/// `createLinearGradient`.
unsafe fn register_gradient_class(ctx: *mut sys::JSContext) {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new(GRADIENT_CLASS_KIND).unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(gradient_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, GRADIENT_CLASS_KIND, &def);
    let existing = sys::JS_GetClassProto(ctx, class_id);
    let already_registered = existing.tag == sys::JS_TAG_OBJECT;
    sys::JS_FreeValue(ctx, existing);
    if already_registered {
        return;
    }
    let proto = sys::JS_NewObject(ctx);
    define_method(ctx, proto, "addColorStop", add_color_stop, 2);
    sys::JS_SetClassProto(ctx, class_id, proto);
}

/// Shared by [`create_linear_gradient`]/[`create_radial_gradient`] -
/// builds a new `CanvasGradient` JS object wrapping `kind`, no stops yet.
unsafe fn make_gradient_object(ctx: *mut sys::JSContext, kind: GradientKind) -> sys::JSValue {
    register_gradient_class(ctx);
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::class_id_for(rt, GRADIENT_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    let data = GradientData {
        kind,
        stops: Vec::new(),
    };
    sys::JS_SetOpaque(
        obj,
        Box::into_raw(Box::new(RefCell::new(data))) as *mut c_void,
    );
    obj
}

/// `ctx.createLinearGradient(x0, y0, x1, y1)` - returns a new
/// `CanvasGradient` (see [`GradientData`]) with no stops yet; real color
/// only takes effect once it's assigned to `ctx.fillStyle` (see
/// `fill_style_set`) after at least one `addColorStop` call.
unsafe extern "C" fn create_linear_gradient(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 4 {
        return sys::js_null();
    }
    let (x0, y0, x1, y1) = (
        read_js_f32(*argv),
        read_js_f32(*argv.add(1)),
        read_js_f32(*argv.add(2)),
        read_js_f32(*argv.add(3)),
    );
    make_gradient_object(ctx, GradientKind::Linear { x0, y0, x1, y1 })
}

/// `ctx.createRadialGradient(x0, y0, r0, x1, y1, r1)` - see
/// [`RadialGradient`]'s own doc for the scope cut: only the outer circle
/// (`x1`, `y1`, `r1`) is kept, `x0`/`y0`/`r0` are accepted but ignored.
unsafe extern "C" fn create_radial_gradient(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 6 {
        return sys::js_null();
    }
    let (cx, cy, radius) = (
        read_js_f32(*argv.add(3)),
        read_js_f32(*argv.add(4)),
        read_js_f32(*argv.add(5)),
    );
    make_gradient_object(ctx, GradientKind::Radial { cx, cy, radius })
}

unsafe extern "C" fn stroke_style_get(
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
        (*ptr).borrow().stroke_style()
    };
    let s = format_hex_color(color);
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

/// `ctx.drawImage(source, dx, dy)` — scoped to just this 3-arg form
/// (real spec also has a `dw`/`dh` scaling overload and a 9-arg source-
/// rectangle overload, neither wired here) and to a `<canvas>` element as
/// `source` (real spec also accepts `HTMLImageElement`/`HTMLVideoElement`/
/// `ImageBitmap`/`OffscreenCanvas` - none of those are wired here yet).
/// The source canvas must already have an active 2D context (i.e.
/// `getContext('2d')` was called on it at least once, registering it in
/// `HostState::canvases` - see `get_context`'s own doc) or this silently
/// does nothing, matching this crate's general "best-effort, no separate
/// error path" convention. Implemented as a full-canvas
/// `get_image_data`/`put_image_data` round trip - no new `render::Canvas2D`
/// API needed, both primitives already exist.
unsafe extern "C" fn draw_image(
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

unsafe extern "C" fn stroke_style_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            if let Some(color) = parse_hex_color(s.trim()) {
                (*ptr).borrow_mut().set_stroke_style(color);
            }
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn line_width_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    let width = if ptr.is_null() {
        1.0
    } else {
        (*ptr).borrow().line_width()
    };
    sys::js_float64(width as f64)
}

unsafe extern "C" fn line_width_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        let width = read_js_f32(val);
        if width > 0.0 {
            (*ptr).borrow_mut().set_line_width(width);
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
    define_method(ctx, proto, "strokeRect", stroke_rect, 4);
    define_method(ctx, proto, "save", save, 0);
    define_method(ctx, proto, "restore", restore, 0);
    define_method(ctx, proto, "translate", translate, 2);
    define_method(ctx, proto, "getImageData", get_image_data, 4);
    define_method(ctx, proto, "putImageData", put_image_data, 3);
    define_method(
        ctx,
        proto,
        "createLinearGradient",
        create_linear_gradient,
        4,
    );
    define_method(
        ctx,
        proto,
        "createRadialGradient",
        create_radial_gradient,
        6,
    );
    define_method(ctx, proto, "drawImage", draw_image, 3);
    define_method(ctx, proto, "beginPath", begin_path, 0);
    define_method(ctx, proto, "moveTo", move_to, 2);
    define_method(ctx, proto, "lineTo", line_to, 2);
    define_method(ctx, proto, "closePath", close_path, 0);
    define_method(ctx, proto, "fill", fill, 0);
    define_method(ctx, proto, "fillText", fill_text, 3);
    define_method(ctx, proto, "strokeText", stroke_text, 3);
    define_method(ctx, proto, "measureText", measure_text, 1);
    define_getter_setter(ctx, proto, "font", font_get as Getter, font_set as Setter);
    define_getter_setter(
        ctx,
        proto,
        "fillStyle",
        fill_style_get as Getter,
        fill_style_set as Setter,
    );
    define_getter_setter(
        ctx,
        proto,
        "strokeStyle",
        stroke_style_get as Getter,
        stroke_style_set as Setter,
    );
    define_getter_setter(
        ctx,
        proto,
        "lineWidth",
        line_width_get as Getter,
        line_width_set as Setter,
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

/// `canvas.toDataURL()` — a real spec method on `HTMLCanvasElement`
/// itself (not `CanvasRenderingContext2D`). Requires `getContext('2d')` to
/// have already been called on this element at least once (i.e. it's
/// registered in `HostState::canvases`) - a canvas nothing has ever drawn
/// through returns `""` rather than a data URL for an all-transparent
/// image, a documented gap (real spec would still produce one). Ignores
/// any `type`/`quality` arguments - see `render::Canvas2D::to_data_url`'s
/// own doc for why the output is always PNG.
unsafe extern "C" fn to_data_url(
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
unsafe extern "C" fn to_blob(
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
