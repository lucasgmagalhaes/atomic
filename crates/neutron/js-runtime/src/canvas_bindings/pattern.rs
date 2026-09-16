//! `CanvasPattern` - the backing for `ctx.createPattern(image, repetition)`.
//! Unlike `CanvasGradient` (whose backing color data resolves lazily, at
//! `ctx.fillStyle =` assignment time - see `gradient::resolve_gradient`),
//! this resolves eagerly at `createPattern` call time: the source
//! canvas's current pixels are copied into the returned object right
//! away, same "immediate copy, not a live reference" convention
//! `drawImage`'s own canvas-to-canvas copy already uses.
use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

use render::Pattern;

use crate::dom_bindings::node_id;

pub(super) const PATTERN_CLASS_KIND: &str = "CanvasPattern";

pub(super) unsafe fn pattern_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut RefCell<Pattern> {
    sys::JS_GetOpaque(
        this_val,
        crate::class_registry::class_id_for(rt, PATTERN_CLASS_KIND),
    ) as *mut RefCell<Pattern>
}

unsafe extern "C" fn pattern_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = pattern_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

/// Registers the `CanvasPattern` class (if not already done) - like
/// `CanvasGradient`, never `new`-able; only reachable via `createPattern`.
/// No methods on the prototype - real spec's own `CanvasPattern.setTransform`
/// isn't wired (this crate's pattern tiling doesn't go through the
/// transform matrix at all, see `render::canvas::patterns`'s own doc).
unsafe fn register_pattern_class(ctx: *mut sys::JSContext) {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new(PATTERN_CLASS_KIND).unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(pattern_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, PATTERN_CLASS_KIND, &def);
    let existing = sys::JS_GetClassProto(ctx, class_id);
    let already_registered = existing.tag == sys::JS_TAG_OBJECT;
    sys::JS_FreeValue(ctx, existing);
    if already_registered {
        return;
    }
    let proto = sys::JS_NewObject(ctx);
    sys::JS_SetClassProto(ctx, class_id, proto);
}

/// `ctx.createPattern(source, repetition)` - `source` scoped to a
/// `<canvas>` element that already has an active 2D context (same
/// `HostState::canvases` lookup `drawImage`'s own canvas-source case
/// uses), `repetition` accepted but see `render::canvas::patterns`'s own
/// doc for why only `'repeat'` tiling is real. Returns `null` when the
/// source isn't a canvas with an active context, matching this crate's
/// general "best-effort, no separate error path" convention.
pub(super) unsafe extern "C" fn create_pattern(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_null();
    }
    let Some(source_node) = node_id(ctx, *argv) else {
        return sys::js_null();
    };
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return sys::js_null();
    }
    let Some(source_backing) = (*state).canvases.get(&source_node).cloned() else {
        return sys::js_null();
    };
    let (source_width, source_height, source_pixels) = {
        let source = source_backing.borrow();
        (source.width(), source.height(), source.get_image_data())
    };

    register_pattern_class(ctx);
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::class_id_for(rt, PATTERN_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    let pattern = Pattern {
        source_width,
        source_height,
        source_pixels,
    };
    sys::JS_SetOpaque(
        obj,
        Box::into_raw(Box::new(RefCell::new(pattern))) as *mut c_void,
    );
    obj
}
