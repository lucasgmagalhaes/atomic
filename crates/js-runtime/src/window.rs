//! `window` — this engine has exactly one browsing context (no iframes
//! anywhere in `dom`), so `window` is simply an alias for the global object
//! itself, matching how a real top-level `window` *is* `globalThis` in its
//! own realm. `self`/`top`/`parent` all alias the same object too, since
//! there is no frame tree to point them at anything else.
use quickjs_sys as sys;
use std::ffi::CString;

unsafe fn alias(ctx: *mut sys::JSContext, global: sys::JSValue, name: &str) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, global, cname.as_ptr(), sys::JS_DupValue(ctx, global));
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    alias(ctx, global, "window");
    alias(ctx, global, "self");
    alias(ctx, global, "top");
    alias(ctx, global, "parent");
    crate::events::define_simple_event_target(ctx, global);
    sys::JS_FreeValue(ctx, global);

    // Real `document` is a `Node` (`Document extends Node`) in a real DOM,
    // so it gets `EventTarget` for free from that hierarchy. This crate's
    // `document` global is a plain object, not a `Node`-class instance (see
    // `crate::document`'s own docs), so it needs the same non-tree-walking
    // `addEventListener`/`dispatchEvent` surface `window` just got, wired
    // here rather than in `crate::document::get_or_create` itself since
    // that function is called repeatedly (idempotent get-or-create) and
    // redefining these methods on every call would be wasteful, not just
    // redundant.
    let document = crate::document::get_or_create(ctx);
    crate::events::define_simple_event_target(ctx, document);
    sys::JS_FreeValue(ctx, document);
}
