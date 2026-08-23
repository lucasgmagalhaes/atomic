//! `on(name, callback)` — the host-driven half of the mockup's automation
//! surface (watchdog reconnect, claim-idle triggers, ...). Not a DOM event:
//! `AutomationEngine`'s context has no `dom::Dom`, so there's nothing to
//! collide with `js-runtime`'s own `dispatchEvent`. Registry keyed by
//! `JSContext` pointer, same thread_local convention as `timers`/
//! `fetch_async` in js-runtime — see that crate's `timers.rs` for why.
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

thread_local! {
    static LISTENERS: RefCell<HashMap<usize, HashMap<String, Vec<sys::JSValue>>>> =
        RefCell::new(HashMap::new());
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

unsafe extern "C" fn on(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return sys::js_undefined();
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let callback = sys::JS_DupValue(ctx, *argv.add(1));
    LISTENERS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .entry(name)
            .or_default()
            .push(callback);
    });
    sys::js_undefined()
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    let name_c = CString::new("on").unwrap();
    let f = sys::JS_NewCFunction2(ctx, on, name_c.as_ptr(), 2, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, global, name_c.as_ptr(), f);
    sys::JS_FreeValue(ctx, global);
}

/// Calls every listener registered for `name` on `ctx`, in registration
/// order. Listeners aren't removed — there's no `off()` yet, matching this
/// pass's "wiring, not full coverage" scope (see this crate's top-level
/// doc).
pub(crate) unsafe fn emit(ctx: *mut sys::JSContext, name: &str) {
    let callbacks = LISTENERS.with(|reg| {
        reg.borrow()
            .get(&(ctx as usize))
            .and_then(|m| m.get(name))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|cb| sys::JS_DupValue(ctx, cb))
            .collect::<Vec<_>>()
    });
    for callback in callbacks {
        let result = sys::JS_Call(ctx, callback, sys::js_undefined(), 0, std::ptr::null_mut());
        sys::JS_FreeValue(ctx, result);
        sys::JS_FreeValue(ctx, callback);
    }
}

/// Frees every still-registered listener for `ctx`. Must run before
/// `JS_FreeContext(ctx)`.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    let Some(by_name) = LISTENERS.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) else {
        return;
    };
    for (_, callbacks) in by_name {
        for callback in callbacks {
            sys::JS_FreeValue(ctx, callback);
        }
    }
}
