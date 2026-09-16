//! `CanvasGradient` - the shared backing for `ctx.createLinearGradient`/
//! `ctx.createRadialGradient`/`ctx.createConicGradient`, and `fillStyle`'s
//! gradient-object branch (see `super::fill_stroke::fill_style_set`).
use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

use layout_engine::Color;
use render::{ConicGradient, FillGradient, LinearGradient, RadialGradient};

use crate::js_helpers::define_method;

use super::helpers::{parse_hex_color, read_js_f32, read_js_string};

pub(super) const GRADIENT_CLASS_KIND: &str = "CanvasGradient";

/// `ctx.createLinearGradient(...)`/`ctx.createRadialGradient(...)`'s
/// shared backing - one `CanvasGradient` class covers both (real spec's
/// own shape too: both factories return the same `CanvasGradient` type).
/// `addColorStop` just accumulates `(offset, color)` pairs regardless of
/// `kind`; [`resolve_gradient`] (called when this object is actually
/// assigned to `ctx.fillStyle`) is what reduces them down to the 2-stop
/// shape `render::Canvas2D` itself supports (see `render::LinearGradient`/
/// `render::RadialGradient`'s own docs on why only 2 effective stops).
enum GradientKind {
    Linear { x0: f32, y0: f32, x1: f32, y1: f32 },
    Radial { cx: f32, cy: f32, radius: f32 },
    Conic { start_angle: f32, cx: f32, cy: f32 },
}

pub(super) struct GradientData {
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
pub(super) fn resolve_gradient(data: &GradientData) -> Option<FillGradient> {
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
        GradientKind::Conic {
            start_angle,
            cx,
            cy,
        } => FillGradient::Conic(ConicGradient {
            start_angle,
            cx,
            cy,
            start,
            end,
        }),
    })
}

pub(super) unsafe fn gradient_opaque(
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
/// `#rgb`/`#rrggbb` hex forms `helpers::parse_hex_color` does elsewhere.
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
/// `createLinearGradient`/`createRadialGradient`.
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
/// `super::fill_stroke::fill_style_set`) after at least one
/// `addColorStop` call.
pub(super) unsafe extern "C" fn create_linear_gradient(
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
/// `render::RadialGradient`'s own doc for the scope cut: only the outer
/// circle (`x1`, `y1`, `r1`) is kept, `x0`/`y0`/`r0` are accepted but
/// ignored.
pub(super) unsafe extern "C" fn create_radial_gradient(
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

/// `ctx.createConicGradient(startAngle, x, y)` - `startAngle` in radians,
/// matching real spec.
pub(super) unsafe extern "C" fn create_conic_gradient(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 3 {
        return sys::js_null();
    }
    let (start_angle, cx, cy) = (
        read_js_f32(*argv),
        read_js_f32(*argv.add(1)),
        read_js_f32(*argv.add(2)),
    );
    make_gradient_object(
        ctx,
        GradientKind::Conic {
            start_angle,
            cx,
            cy,
        },
    )
}
