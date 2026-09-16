//! Real `HTMLCanvasElement.getContext('2d')` (`spec/matrix/browser-apis.md`'s
//! Canvas2D gap): wires the already-existing, GPU-backed `render::Canvas2D`
//! (`fillRect`/`clearRect`/`fillStyle`/`strokeRect`/`strokeStyle`/
//! `lineWidth`/`save`/`restore`/`translate`/`createLinearGradient`/
//! `createRadialGradient` — see that module's own scope-cut doc, which
//! this binding inherits unchanged) into JavaScript and into this
//! worker's real page compositing. Also adds `drawImage(source, dx, dy)`
//! - canvas-to-canvas only (see `image_data::draw_image`'s own doc),
//! built entirely from `get_image_data`/`put_image_data` with no new
//! `render::Canvas2D` API - and `beginPath`/`moveTo`/`lineTo`/
//! `closePath`/`fill` (convex polygons only, no stroke, no curves - see
//! `render::Canvas2D::fill`'s own doc) - plus real `ctx.font`,
//! `fillText`/`strokeText`/`measureText` (no `maxWidth` wrapping,
//! `strokeText` isn't a real outline stroke - see
//! `render::Canvas2D::fill_text`/`stroke_text`/`set_font`'s own docs).
//! Also adds `toDataURL()`/`toBlob()` on `HTMLCanvasElement` itself (see
//! `element::to_data_url`/`element::to_blob`'s own docs - real PNG
//! encoding, requires an already-active 2D context; `toBlob`'s callback
//! runs synchronously, not queued as a real spec-shaped async task).
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
//!
//! Split into one file per concern: `helpers` (JS-value read/write
//! utilities shared everywhere), `gradient` (`CanvasGradient`),
//! `fill_stroke` (`fillRect`/`clearRect`/`strokeRect`/`fillStyle`/
//! `strokeStyle`/`lineWidth`), `image_data` (`getImageData`/
//! `putImageData`/`drawImage`), `path` (convex filled paths), `text`
//! (`ctx.font`/text methods), `state` (`save`/`restore`/`translate`),
//! `element` (`HTMLCanvasElement`'s own `getContext`/`toDataURL`/
//! `toBlob`). `context2d_opaque`/`context2d_finalizer` and
//! `CONTEXT_2D_CLASS_KIND` live here since every submodule needs them;
//! `register()` is the one place that assembles every submodule's
//! methods onto `CanvasRenderingContext2D.prototype`.
use std::cell::RefCell;
use std::ffi::CString;
use std::rc::Rc;

use quickjs_sys as sys;

use render::Canvas2D;

use crate::js_helpers::{define_getter_setter, define_method, Getter, Setter};

mod element;
mod fill_stroke;
mod gradient;
mod helpers;
mod image_data;
mod path;
mod state;
mod text;

pub(crate) use element::define_canvas_properties;

const CONTEXT_2D_CLASS_KIND: &str = "CanvasRenderingContext2D";

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
    define_method(ctx, proto, "fillRect", fill_stroke::fill_rect, 4);
    define_method(ctx, proto, "clearRect", fill_stroke::clear_rect, 4);
    define_method(ctx, proto, "strokeRect", fill_stroke::stroke_rect, 4);
    define_method(ctx, proto, "save", state::save, 0);
    define_method(ctx, proto, "restore", state::restore, 0);
    define_method(ctx, proto, "translate", state::translate, 2);
    define_method(ctx, proto, "getImageData", image_data::get_image_data, 4);
    define_method(ctx, proto, "putImageData", image_data::put_image_data, 3);
    define_method(
        ctx,
        proto,
        "createLinearGradient",
        gradient::create_linear_gradient,
        4,
    );
    define_method(
        ctx,
        proto,
        "createRadialGradient",
        gradient::create_radial_gradient,
        6,
    );
    define_method(ctx, proto, "drawImage", image_data::draw_image, 3);
    define_method(ctx, proto, "beginPath", path::begin_path, 0);
    define_method(ctx, proto, "moveTo", path::move_to, 2);
    define_method(ctx, proto, "lineTo", path::line_to, 2);
    define_method(ctx, proto, "arc", path::arc, 5);
    define_method(ctx, proto, "bezierCurveTo", path::bezier_curve_to, 6);
    define_method(ctx, proto, "quadraticCurveTo", path::quadratic_curve_to, 4);
    define_method(ctx, proto, "closePath", path::close_path, 0);
    define_method(ctx, proto, "fill", path::fill, 0);
    define_method(ctx, proto, "fillText", text::fill_text, 3);
    define_method(ctx, proto, "strokeText", text::stroke_text, 3);
    define_method(ctx, proto, "measureText", text::measure_text, 1);
    define_getter_setter(
        ctx,
        proto,
        "font",
        text::font_get as Getter,
        text::font_set as Setter,
    );
    define_getter_setter(
        ctx,
        proto,
        "fillStyle",
        fill_stroke::fill_style_get as Getter,
        fill_stroke::fill_style_set as Setter,
    );
    define_getter_setter(
        ctx,
        proto,
        "strokeStyle",
        fill_stroke::stroke_style_get as Getter,
        fill_stroke::stroke_style_set as Setter,
    );
    define_getter_setter(
        ctx,
        proto,
        "lineWidth",
        fill_stroke::line_width_get as Getter,
        fill_stroke::line_width_set as Setter,
    );
    sys::JS_SetClassProto(ctx, class_id, proto);
}
