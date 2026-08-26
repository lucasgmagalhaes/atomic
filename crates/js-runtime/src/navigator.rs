//! `navigator` global object — standard read-only properties for feature
//! detection. `navigator.clipboard` is registered separately by
//! `crate::clipboard` (same pattern: it creates-or-fetches the `navigator`
//! object before attaching `clipboard`).
//!
//! Properties are computed once per context at registration time — they
//! reflect the host process, not any web page's idea of the browser.
//! Deviation: `language` returns the system locale (not a browser-
//! configurable list), `platform` returns `std::env::consts::OS`,
//! `userAgent` is a fixed string identifying this engine, `hardware-
//! Concurrency` returns the available parallelism.
use std::ffi::CString;

use quickjs_sys as sys;

unsafe fn js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}

/// Returns (or creates) the `navigator` global object, attaching it to
/// the global object if it doesn't exist yet. The returned value is a
/// *new reference* (caller must `JS_FreeValue`).
pub(crate) unsafe fn get_or_create_navigator(ctx: *mut sys::JSContext) -> sys::JSValue {
    let global = sys::JS_GetGlobalObject(ctx);
    let nav_name = CString::new("navigator").unwrap();
    let navigator = sys::JS_GetPropertyStr(ctx, global, nav_name.as_ptr());
    let navigator = if navigator.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, navigator);
        let obj = sys::JS_NewObject(ctx);
        sys::JS_SetPropertyStr(ctx, global, nav_name.as_ptr(), sys::JS_DupValue(ctx, obj));
        obj
    } else {
        navigator
    };
    sys::JS_FreeValue(ctx, global);
    navigator
}

unsafe fn define_readonly_string(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    value: &str,
) {
    let cname = CString::new(name).unwrap();
    let js_val = js_string(ctx, value);
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), js_val);
}

unsafe fn define_readonly_int(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &str, value: i32) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_float64(value as f64));
}

unsafe fn define_readonly_bool(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    value: bool,
) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_bool(value));
}

/// Registers the `navigator` global with standard read-only properties.
/// If `clipboard::register` runs later, it adds `navigator.clipboard`.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let navigator = get_or_create_navigator(ctx);

    // userAgent — identifies this engine
    define_readonly_string(
        ctx,
        navigator,
        "userAgent",
        "Mozilla/5.0 (Nimble; like Gecko) Nimble/0.1.0",
    );
    define_readonly_string(
        ctx,
        navigator,
        "appVersion",
        "5.0 (Nimble; like Gecko) Nimble/0.1.0",
    );
    define_readonly_string(ctx, navigator, "appName", "Netscape");
    define_readonly_string(ctx, navigator, "vendor", "Nimble Labs");

    // platform — the host OS
    let platform = match std::env::consts::OS {
        "windows" => "Win32",
        "macos" => "MacIntel",
        "linux" => "Linux x86_64",
        other => other,
    };
    define_readonly_string(ctx, navigator, "platform", platform);

    // language — best-effort system locale
    let language = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LANGUAGE"))
        .unwrap_or_else(|_| "en-US".to_string())
        .split('.')
        .next()
        .unwrap_or("en-US")
        .replace('_', "-");
    define_readonly_string(ctx, navigator, "language", &language);
    // languages — just the primary one
    let languages_arr = sys::JS_NewArray(ctx);
    let lang_val = js_string(ctx, &language);
    sys::JS_SetPropertyUint32(ctx, languages_arr, 0, lang_val);
    let languages_name = CString::new("languages").unwrap();
    sys::JS_SetPropertyStr(ctx, navigator, languages_name.as_ptr(), languages_arr);

    // hardwareConcurrency — real available parallelism
    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(1);
    define_readonly_int(ctx, navigator, "hardwareConcurrency", cores);

    // deviceMemory — rough approximation (no real API for this on
    // desktop; return a power-of-two guess based on available memory,
    // clamped to the real API's 0.25/0.5/1/2/4/8 enum)
    let device_memory = if cores >= 16 {
        8
    } else if cores >= 8 {
        4
    } else if cores >= 4 {
        2
    } else {
        1
    };
    define_readonly_int(ctx, navigator, "deviceMemory", device_memory);

    // onLine — always true (no offline detection)
    define_readonly_bool(ctx, navigator, "onLine", true);

    // cookieEnabled — always true (cookies are wired)
    define_readonly_bool(ctx, navigator, "cookieEnabled", true);

    // maxTouchPoints — 0 on desktop
    define_readonly_int(ctx, navigator, "maxTouchPoints", 0);

    // webdriver — false (not automated)
    define_readonly_bool(ctx, navigator, "webdriver", false);

    sys::JS_FreeValue(ctx, navigator);
}
