//! `fetchSync(url)`: a synchronous global backed by `net::get`.
//!
//! Deviation from the spec: real `fetch` is Promise-based and non-blocking.
//! There's no event loop or microtask queue in this runtime yet (same gap
//! documented for `performance.now`'s per-document origin and timers/rAF —
//! all deferred to phase 4's `profile`/`ipc` process, where a real per-tab
//! run loop will exist to drive them). Until then this blocks the calling
//! thread on the request and returns a plain result object instead of a
//! `Promise`: `{ ok, status, body }`, where `body` is the response bytes
//! decoded as UTF-8 (lossily — binary responses aren't representable
//! without `ArrayBuffer`/`Uint8Array` support on the return path, which
//! doesn't exist here). On network/TLS/URL failure, returns
//! `{ ok: false, status: 0, body: "" }` with the error swallowed rather
//! than thrown — no exception-construction helper is bound yet either.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

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

unsafe extern "C" fn fetch_sync(
  ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return crate::request_response::create_response_object(ctx, 0, &[], Some(Vec::new()), "");
  }

  let Some(url) = read_js_string(ctx, *argv) else {
    return crate::request_response::create_response_object(ctx, 0, &[], Some(Vec::new()), "");
  };

  if crate::cors::is_mixed_content_blocked(crate::cors::page_origin(ctx).as_deref(), &url)
    || crate::csp::is_request_blocked(ctx, &url)
  {
    return crate::request_response::create_response_object(ctx, 0, &[], Some(Vec::new()), &url);
  }

  let mut spec = if argc >= 2 {
    crate::fetch_async::read_request_init(ctx, *argv.add(1))
  } else {
    crate::fetch_async::RequestSpec {
      method: "GET".to_string(),
      headers: Vec::new(),
      body: None,
    }
  };
  if let Some(referrer) = crate::cors::referrer_header(ctx, &url) {
    spec.headers.push(("Referer".to_string(), referrer));
  }
  let crate::fetch_async::RequestSpec {
    method,
    headers,
    body,
  } = spec;
  let page_origin = crate::cors::page_origin(ctx);
  match net::request(&method, &url, &headers, body) {
    Ok(response)
      if crate::cors::is_response_allowed(page_origin.as_deref(), &url, &response.headers) =>
    {
      crate::request_response::create_response_object(
        ctx,
        response.status,
        &response.headers,
        Some(response.body),
        &url,
      )
    }
    Ok(_) | Err(_) => {
      crate::request_response::create_response_object(ctx, 0, &[], Some(Vec::new()), &url)
    }
  }
}

/// Registers `fetchSync` as a global on `ctx`.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
  let global = sys::JS_GetGlobalObject(ctx);

  let name = CString::new("fetchSync").unwrap();
  let f = sys::JS_NewCFunction2(ctx, fetch_sync, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
  sys::JS_SetPropertyStr(ctx, global, name.as_ptr(), f);

  sys::JS_FreeValue(ctx, global);
}
