//! Minimal `performance.now()`: registers a `performance` global object
//! with a `now()` method returning milliseconds (sub-ms precision) since
//! this process started.
//!
//! Deviation from the spec: `performance.now()`'s time origin is normally
//! per-document (when that document's navigation started), not per-process.
//! A per-process origin is what's available without a document/navigation
//! concept yet (that's phase 3+); revisit once `profile` gives each tab its
//! own process and a real navigation start to anchor on.
use std::os::raw::c_int;
use std::sync::OnceLock;
use std::time::Instant;

use quickjs_sys as sys;

static PROCESS_START: OnceLock<Instant> = OnceLock::new();

unsafe extern "C" fn now(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let start = PROCESS_START.get_or_init(Instant::now);
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    sys::js_float64(ms)
}

/// Registers `performance.now` as a global on `ctx`.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    use std::ffi::CString;

    let global = sys::JS_GetGlobalObject(ctx);
    let performance = sys::JS_NewObject(ctx);

    let now_name = CString::new("now").unwrap();
    let now_fn = sys::JS_NewCFunction2(ctx, now, now_name.as_ptr(), 0, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, performance, now_name.as_ptr(), now_fn);

    let performance_name = CString::new("performance").unwrap();
    sys::JS_SetPropertyStr(ctx, global, performance_name.as_ptr(), performance);

    sys::JS_FreeValue(ctx, global);
}
