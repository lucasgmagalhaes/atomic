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
  url: String,
  receiver: mpsc::Receiver<Result<net::Response, net::Error>>,
}

/// A fully-owned request description handed to the background thread —
/// nothing in it borrows from JS-land. Fields are read by `fetch_sync`
/// too, so they're crate-visible.
pub(crate) struct RequestSpec {
  pub(crate) method: String,
  pub(crate) headers: Vec<(String, String)>,
  pub(crate) body: Option<Vec<u8>>,
}

struct PendingXhr {
  xhr_obj: sys::JSValue,
  url: String,
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

unsafe fn throw_type_error(ctx: *mut sys::JSContext, message: &str) -> sys::JSValue {
  sys::JS_Throw(ctx, new_js_string(ctx, message))
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
unsafe fn call_if_present(
  ctx: *mut sys::JSContext,
  callback: sys::JSValue,
  this_val: sys::JSValue,
  mut arg: sys::JSValue,
) {
  if callback.tag != sys::JS_TAG_UNDEFINED {
    let result = sys::JS_Call(ctx, callback, this_val, 1, &mut arg);
    sys::JS_FreeValue(ctx, result);
  }
  sys::JS_FreeValue(ctx, arg);
  sys::JS_FreeValue(ctx, callback);
}

unsafe fn build_response_object(
  ctx: *mut sys::JSContext,
  response: &net::Response,
  url: &str,
) -> sys::JSValue {
  crate::request_response::create_response_object(
    ctx,
    response.status,
    &response.headers,
    Some(response.body.clone()),
    url,
  )
}

/// Reads the per-spec `RequestInit` subset this engine supports:
/// `method` (uppercased, default `"GET"`), `headers` (a plain object of
/// name → string value pairs), and `body` (a string, or a FormData
/// instance serialized to multipart bytes with its boundary `Content-Type`
/// auto-set unless the caller already supplied one). Unknown options and
/// non-string header values are silently ignored (documented deviation).
/// Pure JS-value parsing with no side effects, so `fetch_sync` reuses it.
pub(crate) unsafe fn read_request_init(
  ctx: *mut sys::JSContext,
  init: sys::JSValue,
) -> RequestSpec {
  let mut method = "GET".to_string();
  let mut headers: Vec<(String, String)> = Vec::new();
  let mut body: Option<Vec<u8>> = None;
  if init.tag != sys::JS_TAG_OBJECT {
    return RequestSpec {
      method,
      headers,
      body,
    };
  }
  let method_val = get_prop(ctx, init, "method");
  if let Some(text) = read_js_string(ctx, method_val) {
    if !text.is_empty() {
      method = text.to_ascii_uppercase();
    }
  }
  sys::JS_FreeValue(ctx, method_val);

  // Caller headers first so a FormData's derived Content-Type can't
  // clobber one the script set explicitly.
  let headers_val = get_prop(ctx, init, "headers");
  if headers_val.tag == sys::JS_TAG_OBJECT {
    let mut tab: *mut sys::JSPropertyEnum = std::ptr::null_mut();
    let mut len: u32 = 0;
    if sys::JS_GetOwnPropertyNames(
      ctx,
      &mut tab,
      &mut len,
      headers_val,
      sys::JS_GPN_STRING_ENUM,
    ) >= 0
      && !tab.is_null()
    {
      for entry in std::slice::from_raw_parts(tab, len as usize) {
        let mut name_len: usize = 0;
        let name_ptr = sys::JS_AtomToCStringLen(ctx, &mut name_len, entry.atom);
        if name_ptr.is_null() {
          continue;
        }
        let bytes = std::slice::from_raw_parts(name_ptr as *const u8, name_len);
        let key = String::from_utf8_lossy(bytes).into_owned();
        sys::JS_FreeCString(ctx, name_ptr);
        let prop_val = sys::JS_GetProperty(ctx, headers_val, entry.atom);
        if let Some(value) = read_js_string(ctx, prop_val) {
          headers.push((key, value));
        }
        sys::JS_FreeValue(ctx, prop_val);
      }
      sys::JS_FreePropertyEnum(ctx, tab, len);
    }
  }
  sys::JS_FreeValue(ctx, headers_val);

  let body_val = get_prop(ctx, init, "body");
  match body_val.tag {
    sys::JS_TAG_STRING => {
      if let Some(text) = read_js_string(ctx, body_val) {
        // Per-spec default content type for a string body.
        if !headers
          .iter()
          .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        {
          headers.push((
            "Content-Type".to_string(),
            "text/plain;charset=UTF-8".to_string(),
          ));
        }
        body = Some(text.into_bytes());
      }
    }
    sys::JS_TAG_OBJECT => {
      if let Some(serialized) = crate::form_data::serialize(ctx, body_val) {
        if !headers
          .iter()
          .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        {
          headers.push(("Content-Type".to_string(), serialized.content_type));
        }
        body = Some(serialized.body);
      }
    }
    _ => {}
  }
  sys::JS_FreeValue(ctx, body_val);

  RequestSpec {
    method,
    headers,
    body,
  }
}

unsafe extern "C" fn fetch(
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

unsafe extern "C" fn xhr_constructor(
  ctx: *mut sys::JSContext,
  new_target: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
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

unsafe extern "C" fn xhr_open(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc >= 2 {
    if let Some(method) = read_js_string(ctx, *argv) {
      let upper = method.to_ascii_uppercase();
      set_str(ctx, this_val, "_method", &upper);
    }
    if let Some(url) = read_js_string(ctx, *argv.add(1)) {
      set_str(ctx, this_val, "_url", &url);
    }
  }
  set_num(ctx, this_val, "readyState", 1.0);
  sys::js_undefined()
}

/// `setRequestHeader(name, value)` — accumulates into a plain object
/// stored on the XHR instance (`_headers`), applied at `send` time.
unsafe extern "C" fn xhr_set_request_header(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 2 {
    return throw_type_error(ctx, "setRequestHeader expects a name and a value");
  }
  let Some(name) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "header name must be a string");
  };
  let Some(value) = read_js_string(ctx, *argv.add(1)) else {
    return throw_type_error(ctx, "header value must be a string");
  };
  let key = CString::new("_headers").unwrap();
  let mut headers_obj = sys::JS_GetPropertyStr(ctx, this_val, key.as_ptr());
  if headers_obj.tag != sys::JS_TAG_OBJECT {
    sys::JS_FreeValue(ctx, headers_obj);
    headers_obj = sys::JS_NewObject(ctx);
    sys::JS_SetPropertyStr(
      ctx,
      this_val,
      key.as_ptr(),
      sys::JS_DupValue(ctx, headers_obj),
    );
  }
  let cname = CString::new(name).unwrap();
  sys::JS_SetPropertyStr(ctx, headers_obj, cname.as_ptr(), new_js_string(ctx, &value));
  sys::JS_FreeValue(ctx, headers_obj);
  sys::js_undefined()
}

unsafe extern "C" fn xhr_send(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let url_val = get_prop(ctx, this_val, "_url");
  let Some(url) = read_js_string(ctx, url_val) else {
    sys::JS_FreeValue(ctx, url_val);
    return sys::js_undefined();
  };
  sys::JS_FreeValue(ctx, url_val);

  // Method (uppercased at open() time; default GET when open() never
  // ran or stored nothing), script headers, and the optional body.
  let mut method = "GET".to_string();
  let method_val = get_prop(ctx, this_val, "_method");
  if let Some(stored) = read_js_string(ctx, method_val) {
    method = stored;
  }
  sys::JS_FreeValue(ctx, method_val);
  let mut headers: Vec<(String, String)> = Vec::new();
  let headers_obj = get_prop(ctx, this_val, "_headers");
  if headers_obj.tag == sys::JS_TAG_OBJECT {
    let mut tab: *mut sys::JSPropertyEnum = std::ptr::null_mut();
    let mut len: u32 = 0;
    if sys::JS_GetOwnPropertyNames(
      ctx,
      &mut tab,
      &mut len,
      headers_obj,
      sys::JS_GPN_STRING_ENUM,
    ) >= 0
      && !tab.is_null()
    {
      for entry in std::slice::from_raw_parts(tab, len as usize) {
        let mut name_len: usize = 0;
        let name_ptr = sys::JS_AtomToCStringLen(ctx, &mut name_len, entry.atom);
        if name_ptr.is_null() {
          continue;
        }
        let bytes = std::slice::from_raw_parts(name_ptr as *const u8, name_len);
        let key = String::from_utf8_lossy(bytes).into_owned();
        sys::JS_FreeCString(ctx, name_ptr);
        let prop_val = sys::JS_GetProperty(ctx, headers_obj, entry.atom);
        if let Some(value) = read_js_string(ctx, prop_val) {
          headers.push((key, value));
        }
        sys::JS_FreeValue(ctx, prop_val);
      }
      sys::JS_FreePropertyEnum(ctx, tab, len);
    }
  }
  sys::JS_FreeValue(ctx, headers_obj);
  let body: Option<Vec<u8>> = if argc >= 1 {
    match *argv {
      val if val.tag == sys::JS_TAG_STRING => {
        read_js_string(ctx, val).map(|text| text.into_bytes())
      }
      val => crate::form_data::serialize(ctx, val).map(|serialized| {
        if !headers
          .iter()
          .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        {
          headers.push(("Content-Type".to_string(), serialized.content_type));
        }
        serialized.body
      }),
    }
  } else {
    None
  };

  if crate::cors::is_mixed_content_blocked(crate::cors::page_origin(ctx).as_deref(), &url)
    || crate::csp::is_request_blocked(ctx, &url)
  {
    // Fed straight to `pump`'s existing completion handling (never
    // actually sent over the network) so a blocked request goes
    // through the exact same readyState/onerror flow a real network
    // failure would, rather than a separate synchronous error path.
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(Err(net::Error::Request(
      "blocked by mixed-content or Content-Security-Policy".to_string(),
    )));
    let xhr_obj = sys::JS_DupValue(ctx, this_val);
    with_state(ctx, |s| {
      s.xhrs.push(PendingXhr {
        xhr_obj,
        url,
        receiver: rx,
      })
    });
    return sys::js_undefined();
  }

  // Referrer last, same as fetch() — browser-level, not script-settable.
  if let Some(referrer) = crate::cors::referrer_header(ctx, &url) {
    headers.push(("Referer".to_string(), referrer));
  }
  let (tx, rx) = mpsc::channel();
  let request_url = url.clone();
  thread::spawn(move || {
    let _ = tx.send(net::request(&method, &url, &headers, body));
  });

  let xhr_obj = sys::JS_DupValue(ctx, this_val);
  with_state(ctx, |s| {
    s.xhrs.push(PendingXhr {
      xhr_obj,
      url: request_url,
      receiver: rx,
    })
  });
  sys::js_undefined()
}

unsafe fn define_method(
  ctx: *mut sys::JSContext,
  proto: sys::JSValue,
  name: &str,
  func: sys::JSCFunction,
  length: c_int,
) {
  let name_c = CString::new(name).unwrap();
  let f = sys::JS_NewCFunction2(ctx, func, name_c.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
  sys::JS_SetPropertyStr(ctx, proto, name_c.as_ptr(), f);
}

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
