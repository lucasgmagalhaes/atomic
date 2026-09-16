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
//!
//! Split into `types.rs` (pending-fetch/XHR bookkeeping + the per-context
//! state map), `helpers.rs` (shared string/property/callback helpers),
//! `request_init.rs` (`RequestInit` parsing, shared with `fetch_sync`),
//! `fetch_fn.rs` (the `fetch()` global), and `xhr.rs`
//! (`XMLHttpRequest`) — this file keeps the module doc and the three
//! public entry points: `register`, `pump`, `cleanup`.

use quickjs_sys as sys;

use std::ffi::CString;
use std::os::raw::c_int;

use crate::js_helpers::define_method;

mod fetch_fn;
mod helpers;
mod request_init;
mod types;
mod xhr;

pub(crate) use request_init::read_request_init;
pub(crate) use types::RequestSpec;

use fetch_fn::fetch;
use helpers::{build_response_object, call_if_present, get_prop, new_js_string, set_num, set_str};
use types::{with_state, STATE};
use xhr::{xhr_constructor, xhr_open, xhr_send, xhr_set_request_header};

/// Registers `fetch` and the `XMLHttpRequest` constructor as globals.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);

    let fetch_name = CString::new("fetch").unwrap();
    let fetch_fn =
        sys::JS_NewCFunction2(ctx, fetch, fetch_name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, global, fetch_name.as_ptr(), fetch_fn);

    let proto = sys::JS_NewObject(ctx);
    define_method(ctx, proto, "open", xhr_open, 2);
    define_method(ctx, proto, "send", xhr_send, 0);
    define_method(ctx, proto, "setRequestHeader", xhr_set_request_header, 2);

    let ctor_name = CString::new("XMLHttpRequest").unwrap();
    let ctor = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<
            unsafe extern "C" fn(
                *mut sys::JSContext,
                sys::JSValue,
                c_int,
                *mut sys::JSValue,
            ) -> sys::JSValue,
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
    let page_origin = crate::cors::page_origin(ctx);
    for (pending, result) in due_fetches {
        fired += 1;
        let result: Result<net::Response, String> = match result {
            Ok(response)
                if crate::cors::is_response_allowed(
                    page_origin.as_deref(),
                    &pending.url,
                    &response.headers,
                ) =>
            {
                Ok(response)
            }
            Ok(_) => Err("blocked by CORS: no matching Access-Control-Allow-Origin".to_string()),
            Err(e) => Err(e.to_string()),
        };
        match result {
            Ok(response) => {
                let mut obj = build_response_object(ctx, &response, &pending.url);
                let r = sys::JS_Call(ctx, pending.resolve, sys::js_undefined(), 1, &mut obj);
                sys::JS_FreeValue(ctx, r);
                sys::JS_FreeValue(ctx, obj);
            }
            Err(e) => {
                let mut msg = new_js_string(ctx, &e);
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
        let result: Result<net::Response, String> = match result {
            Ok(response)
                if crate::cors::is_response_allowed(
                    page_origin.as_deref(),
                    &pending.url,
                    &response.headers,
                ) =>
            {
                Ok(response)
            }
            Ok(_) => Err("blocked by CORS: no matching Access-Control-Allow-Origin".to_string()),
            Err(e) => Err(e.to_string()),
        };
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
                let msg = new_js_string(ctx, &e);
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
