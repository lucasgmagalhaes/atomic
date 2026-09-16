//! Real async `fetch()` — split out from `fetch_async/mod.rs`.

use std::os::raw::c_int;
use std::sync::mpsc;
use std::thread;

use quickjs_sys as sys;

use super::helpers::{call_if_present, new_js_string, read_js_string};
use super::request_init::read_request_init;
use super::types::{PendingFetch, RequestSpec};
use super::with_state;

pub(super) unsafe extern "C" fn fetch(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let mut resolving_funcs = [sys::js_undefined(); 2];
    let promise = sys::JS_NewPromiseCapability(ctx, resolving_funcs.as_mut_ptr());
    let [resolve, reject] = resolving_funcs;

    let url = if argc >= 1 {
        read_js_string(ctx, *argv)
    } else {
        None
    };
    let Some(url) = url else {
        let msg = new_js_string(ctx, "fetch: missing or invalid url argument");
        call_if_present(ctx, reject, sys::js_undefined(), msg);
        sys::JS_FreeValue(ctx, resolve);
        return promise;
    };
    if crate::cors::is_mixed_content_blocked(crate::cors::page_origin(ctx).as_deref(), &url)
        || crate::csp::is_request_blocked(ctx, &url)
    {
        let msg = new_js_string(
            ctx,
            "fetch: blocked by mixed-content or Content-Security-Policy",
        );
        call_if_present(ctx, reject, sys::js_undefined(), msg);
        sys::JS_FreeValue(ctx, resolve);
        return promise;
    }
    let mut spec = if argc >= 2 {
        read_request_init(ctx, *argv.add(1))
    } else {
        RequestSpec {
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
        }
    };
    // Referrer is a browser-level header — appended last so it can't be
    // overridden by script-supplied headers (matches fetch_sync).
    if let Some(referrer) = crate::cors::referrer_header(ctx, &url) {
        spec.headers.push(("Referer".to_string(), referrer));
    }

    let (tx, rx) = mpsc::channel();
    let request_url = url.clone();
    thread::spawn(move || {
        let _ = tx.send(net::request(
            &spec.method,
            &url,
            &spec.headers,
            spec.body.take(),
        ));
    });

    with_state(ctx, |s| {
        s.fetches.push(PendingFetch {
            resolve,
            reject,
            url: request_url,
            receiver: rx,
        })
    });
    promise
}
