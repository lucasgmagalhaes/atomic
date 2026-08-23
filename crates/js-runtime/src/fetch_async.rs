//! Real async `fetch()` (Promise-based) and `XMLHttpRequest` — both do
//! the actual network request on a real OS thread (same "genuine
//! parallelism, not cooperative" approach `workers` already uses for Web
//! Workers) and hand the result back through [`pump`], which the host
//! must call periodically (same "no real event loop yet" deviation
//! documented on `timers`/`fetchSync` — `pump` is now that module's
//! superset: it also resolves/rejects due fetches, completes due XHRs,
//! and drains quickjs's own Promise reaction job queue via
//! `JS_ExecutePendingJob` so `.then()`/`await` continuations actually
//! run, not just get scheduled).
//!
//! `fetch(url)` returns a real `Promise` (`JS_NewPromiseCapability`), not
//! `fetchSync`'s plain result object — the two coexist: `fetchSync` for
//! callers that can't yield (there's no `await` outside an `async`
//! function without a real microtask-driving loop calling this crate),
//! `fetch` for real Promise-shaped code.
//!
//! `XMLHttpRequest` supports the classic `open(method, url)` / `send()` /
//! `onload`/`onerror` pattern - `method` is accepted but only `GET` is
//! actually implemented (matches `net::get`'s own scope), `readyState`
//! only ever takes the values `0` (UNSENT), `1` (OPENED after `open()`),
//! and `4` (DONE after completion) - the intermediate XHR readyState
//! values (`HEADERS_RECEIVED`, `LOADING`) don't apply since this crate's
//! `net::get` has no streaming/partial-response concept to report them
//! from. No `addEventListener("load", ...)` form (only the `onload`/
//! `onerror` property form) - reusing `events::define_event_target` here
//! is a natural follow-up, not done in this pass.
use std::ffi::CString;
use std::os::raw::c_int;
use std::sync::mpsc;
use std::thread;

use quickjs_sys as sys;

struct PendingFetch {
    resolve: sys::JSValue,
    reject: sys::JSValue,
    receiver: mpsc::Receiver<Result<net::Response, net::Error>>,
}

struct PendingXhr {
    xhr_obj: sys::JSValue,
    receiver: mpsc::Receiver<Result<net::Response, net::Error>>,
}

#[derive(Default)]
struct AsyncState {
    fetches: Vec<PendingFetch>,
    xhrs: Vec<PendingXhr>,
}

thread_local! {
    // Keyed by JSContext pointer, same reasoning as `timers::REGISTRIES`:
    // avoids needing an unsafe Send/Sync impl just to hold `!Send`
    // `JSValue`s behind a plain static.
    static STATE: std::cell::RefCell<std::collections::HashMap<usize, AsyncState>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

fn with_state<R>(ctx: *mut sys::JSContext, f: impl FnOnce(&mut AsyncState) -> R) -> R {
    STATE.with(|s| {
        let mut map = s.borrow_mut();
        let state = map.entry(ctx as usize).or_default();
        f(state)
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

unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe fn set_str(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: &str) {
    let name = CString::new(key).unwrap();
    let js_val = new_js_string(ctx, val);
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), js_val);
}

unsafe fn set_num(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: f64) {
    let name = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), sys::js_float64(val));
}

unsafe fn set_bool(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: bool) {
    let name = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), sys::js_bool(val));
}

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str) -> sys::JSValue {
    let name = CString::new(key).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

/// Calls `callback` with `this = this_val` and one argument, if
/// `callback` is anything other than `undefined` - no `JS_IsFunction`
/// binding exists yet, so a non-function value just fails the call and
/// the resulting exception value is discarded, same "no exception
/// plumbing yet" convention used elsewhere in this crate (e.g. `timers`'s
/// callback firing). `JS_Call`'s `argv` is `JSValueConst*` - borrowed, not
/// consumed - so `arg` (and `callback` itself, also borrowed as
/// `func_obj`) must be freed here regardless of whether the call actually
/// happened.
unsafe fn call_if_present(ctx: *mut sys::JSContext, callback: sys::JSValue, this_val: sys::JSValue, mut arg: sys::JSValue) {
    if callback.tag != sys::JS_TAG_UNDEFINED {
        let result = sys::JS_Call(ctx, callback, this_val, 1, &mut arg);
        sys::JS_FreeValue(ctx, result);
    }
    sys::JS_FreeValue(ctx, arg);
    sys::JS_FreeValue(ctx, callback);
}

unsafe fn build_response_object(ctx: *mut sys::JSContext, response: &net::Response) -> sys::JSValue {
    let body = String::from_utf8_lossy(&response.body).into_owned();
    let obj = sys::JS_NewObject(ctx);
    set_bool(ctx, obj, "ok", (200..300).contains(&response.status));
    set_num(ctx, obj, "status", response.status as f64);
    set_str(ctx, obj, "body", &body);
    obj
}

unsafe extern "C" fn fetch(ctx: *mut sys::JSContext, _this_val: sys::JSValue, argc: c_int, argv: *mut sys::JSValue) -> sys::JSValue {
    let mut resolving_funcs = [sys::js_undefined(); 2];
    let promise = sys::JS_NewPromiseCapability(ctx, resolving_funcs.as_mut_ptr());
    let [resolve, reject] = resolving_funcs;

    let url = if argc >= 1 { read_js_string(ctx, *argv) } else { None };
    let Some(url) = url else {
        let msg = new_js_string(ctx, "fetch: missing or invalid url argument");
        call_if_present(ctx, reject, sys::js_undefined(), msg);
        sys::JS_FreeValue(ctx, resolve);
        return promise;
    };

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(net::get(&url));
    });

    with_state(ctx, |s| s.fetches.push(PendingFetch { resolve, reject, receiver: rx }));
    promise
}

unsafe extern "C" fn xhr_constructor(ctx: *mut sys::JSContext, new_target: sys::JSValue, _argc: c_int, _argv: *mut sys::JSValue) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    let proto = get_prop(ctx, new_target, "prototype");
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    // `JS_SetPrototype` takes `proto` by (const) reference, not
    // ownership - unlike `JS_SetPropertyStr` and friends, it doesn't
    // consume the value, so this call is unconditional either way.
    sys::JS_FreeValue(ctx, proto);
    set_num(ctx, obj, "readyState", 0.0);
    set_num(ctx, obj, "status", 0.0);
    set_str(ctx, obj, "responseText", "");
    obj
}

unsafe extern "C" fn xhr_open(ctx: *mut sys::JSContext, this_val: sys::JSValue, argc: c_int, argv: *mut sys::JSValue) -> sys::JSValue {
    if argc >= 2 {
        if let Some(method) = read_js_string(ctx, *argv) {
            set_str(ctx, this_val, "_method", &method);
        }
        if let Some(url) = read_js_string(ctx, *argv.add(1)) {
            set_str(ctx, this_val, "_url", &url);
        }
    }
    set_num(ctx, this_val, "readyState", 1.0);
    sys::js_undefined()
}

unsafe extern "C" fn xhr_send(ctx: *mut sys::JSContext, this_val: sys::JSValue, _argc: c_int, _argv: *mut sys::JSValue) -> sys::JSValue {
    let url_val = get_prop(ctx, this_val, "_url");
    let Some(url) = read_js_string(ctx, url_val) else {
        sys::JS_FreeValue(ctx, url_val);
        return sys::js_undefined();
    };
    sys::JS_FreeValue(ctx, url_val);

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(net::get(&url));
    });

    let xhr_obj = sys::JS_DupValue(ctx, this_val);
    with_state(ctx, |s| s.xhrs.push(PendingXhr { xhr_obj, receiver: rx }));
    sys::js_undefined()
}

unsafe fn define_method(ctx: *mut sys::JSContext, proto: sys::JSValue, name: &str, func: sys::JSCFunction, length: c_int) {
    let name_c = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(ctx, func, name_c.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, proto, name_c.as_ptr(), f);
}

/// Registers `fetch` and the `XMLHttpRequest` constructor as globals.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);

    let fetch_name = CString::new("fetch").unwrap();
    let fetch_fn = sys::JS_NewCFunction2(ctx, fetch, fetch_name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, global, fetch_name.as_ptr(), fetch_fn);

    let proto = sys::JS_NewObject(ctx);
    define_method(ctx, proto, "open", xhr_open, 2);
    define_method(ctx, proto, "send", xhr_send, 0);

    let ctor_name = CString::new("XMLHttpRequest").unwrap();
    let ctor = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<
            unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue, c_int, *mut sys::JSValue) -> sys::JSValue,
            sys::JSCFunction,
        >(xhr_constructor),
        ctor_name.as_ptr(),
        0,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let prototype_prop = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(ctx, ctor, prototype_prop.as_ptr(), proto);
    sys::JS_SetPropertyStr(ctx, global, ctor_name.as_ptr(), ctor);

    sys::JS_FreeValue(ctx, global);
}

/// Resolves/rejects every fetch whose background request has completed,
/// completes every finished `XMLHttpRequest` (setting `status`/
/// `responseText`/`readyState` and firing `onload`/`onerror`), then
/// drains quickjs's own Promise job queue so any `.then()` continuations
/// those resolutions just triggered actually run. Returns how many
/// fetches/XHRs completed plus how many queued jobs ran.
pub(crate) unsafe fn pump(ctx: *mut sys::JSContext) -> usize {
    let mut fired = 0;

    let due_fetches = with_state(ctx, |s| {
        let mut due = Vec::new();
        let mut i = 0;
        while i < s.fetches.len() {
            match s.fetches[i].receiver.try_recv() {
                Ok(result) => due.push((s.fetches.remove(i), result)),
                Err(_) => i += 1,
            }
        }
        due
    });
    for (pending, result) in due_fetches {
        fired += 1;
        match result {
            Ok(response) => {
                let mut obj = build_response_object(ctx, &response);
                let r = sys::JS_Call(ctx, pending.resolve, sys::js_undefined(), 1, &mut obj);
                sys::JS_FreeValue(ctx, r);
                sys::JS_FreeValue(ctx, obj);
            }
            Err(e) => {
                let mut msg = new_js_string(ctx, &e.to_string());
                let r = sys::JS_Call(ctx, pending.reject, sys::js_undefined(), 1, &mut msg);
                sys::JS_FreeValue(ctx, r);
                sys::JS_FreeValue(ctx, msg);
            }
        }
        sys::JS_FreeValue(ctx, pending.resolve);
        sys::JS_FreeValue(ctx, pending.reject);
    }

    let due_xhrs = with_state(ctx, |s| {
        let mut due = Vec::new();
        let mut i = 0;
        while i < s.xhrs.len() {
            match s.xhrs[i].receiver.try_recv() {
                Ok(result) => due.push((s.xhrs.remove(i), result)),
                Err(_) => i += 1,
            }
        }
        due
    });
    for (pending, result) in due_xhrs {
        fired += 1;
        set_num(ctx, pending.xhr_obj, "readyState", 4.0);
        match result {
            Ok(response) => {
                let body = String::from_utf8_lossy(&response.body).into_owned();
                set_num(ctx, pending.xhr_obj, "status", response.status as f64);
                set_str(ctx, pending.xhr_obj, "responseText", &body);
                let onload = get_prop(ctx, pending.xhr_obj, "onload");
                call_if_present(ctx, onload, pending.xhr_obj, sys::js_undefined());
            }
            Err(e) => {
                let onerror = get_prop(ctx, pending.xhr_obj, "onerror");
                let msg = new_js_string(ctx, &e.to_string());
                call_if_present(ctx, onerror, pending.xhr_obj, msg);
            }
        }
        sys::JS_FreeValue(ctx, pending.xhr_obj);
    }

    let rt = sys::JS_GetRuntime(ctx);
    let mut job_ctx = ctx;
    while sys::JS_IsJobPending(rt) {
        let rc = sys::JS_ExecutePendingJob(rt, &mut job_ctx);
        if rc <= 0 {
            break;
        }
        fired += 1;
    }

    fired
}

/// Frees every still-pending fetch/XHR registration for `ctx`. Must run
/// before `JS_FreeContext(ctx)`.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    let state = STATE.with(|s| s.borrow_mut().remove(&(ctx as usize)));
    let Some(state) = state else { return };
    for pending in state.fetches {
        sys::JS_FreeValue(ctx, pending.resolve);
        sys::JS_FreeValue(ctx, pending.reject);
    }
    for pending in state.xhrs {
        sys::JS_FreeValue(ctx, pending.xhr_obj);
    }
}
