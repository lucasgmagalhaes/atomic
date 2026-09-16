//! `window.screen` — read-only object with display dimensions and
//! color-depth properties. Values are computed once per context at
//! registration time (no live monitor-change detection).
//!
//! Deviations: `width`/`height`/`availWidth`/`availHeight` are hardcoded
//! placeholders (no real monitor enumeration yet — documented same way
//! `page_visibility`'s always-visible state is).
use std::ffi::CString;

use quickjs_sys as sys;

unsafe fn js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let screen = sys::JS_NewObject(ctx);

    let cname = |name: &str| CString::new(name).unwrap();

    // Read the monitor info if available (platform-apis crate has
    // monitor enumeration on some platforms). For now use reasonable
    // desktop defaults — same pattern page_visibility uses.
    let width: i32 = 1920;
    let height: i32 = 1080;

    // width/height
    let c = cname("width");
    sys::JS_SetPropertyStr(ctx, screen, c.as_ptr(), sys::js_float64(width as f64));
    let c = cname("height");
    sys::JS_SetPropertyStr(ctx, screen, c.as_ptr(), sys::js_float64(height as f64));

    // availWidth / availHeight (slightly less than full due to taskbar etc.)
    let avail_width = width;
    let avail_height = height - 40;
    let c = cname("availWidth");
    sys::JS_SetPropertyStr(ctx, screen, c.as_ptr(), sys::js_float64(avail_width as f64));
    let c = cname("availHeight");
    sys::JS_SetPropertyStr(
        ctx,
        screen,
        c.as_ptr(),
        sys::js_float64(avail_height as f64),
    );
    let c = cname("availTop");
    sys::JS_SetPropertyStr(ctx, screen, c.as_ptr(), sys::js_float64(0.0));
    let c = cname("availLeft");
    sys::JS_SetPropertyStr(ctx, screen, c.as_ptr(), sys::js_float64(0.0));

    // colorDepth / pixelDepth
    let c = cname("colorDepth");
    sys::JS_SetPropertyStr(ctx, screen, c.as_ptr(), sys::js_float64(24.0));
    let c = cname("pixelDepth");
    sys::JS_SetPropertyStr(ctx, screen, c.as_ptr(), sys::js_float64(24.0));

    // orientation — basic landscape for default 1920x1080
    let orientation = sys::JS_NewObject(ctx);
    let c = cname("type");
    sys::JS_SetPropertyStr(
        ctx,
        orientation,
        c.as_ptr(),
        js_string(ctx, "landscape-primary"),
    );
    let c = cname("angle");
    sys::JS_SetPropertyStr(ctx, orientation, c.as_ptr(), sys::js_float64(0.0));
    let c = cname("orientation");
    sys::JS_SetPropertyStr(ctx, screen, c.as_ptr(), orientation);

    // Attach to global
    let global = sys::JS_GetGlobalObject(ctx);
    let c = cname("screen");
    sys::JS_SetPropertyStr(ctx, global, c.as_ptr(), screen);
    sys::JS_FreeValue(ctx, global);
}
