//! `URL` constructor and `URLSearchParams` — real URL parsing/resolution
//! backed by the `url` crate (already a dependency for `location` and
//! `cors`). `URLSearchParams` is a full key-value bag with multiple
//! values per key, matching the real API's `append`/`delete`/`get`/
//! `getAll`/`has`/`set`/`sort`/`toString` plus iteration.
//!
//! Deviations: `URLSearchParams` iterator methods return plain arrays
//! (no `Symbol.iterator` support in quickjs-sys); `URL` constructor
//! rejects relative URLs without a base (matching the real spec);
//! `URL.href` setter re-parses the full string (real URL mutation).
#![allow(dead_code)]
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

type Getter = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue) -> sys::JSValue;
type Setter = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue, sys::JSValue) -> sys::JSValue;

unsafe fn js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
  sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
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

unsafe fn get_property(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str) -> sys::JSValue {
  let c = CString::new(key).unwrap();
  sys::JS_GetPropertyStr(ctx, obj, c.as_ptr())
}

unsafe fn set_property(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: sys::JSValue) {
  let c = CString::new(key).unwrap();
  sys::JS_SetPropertyStr(ctx, obj, c.as_ptr(), val);
}

unsafe fn define_method(
  ctx: *mut sys::JSContext,
  obj: sys::JSValue,
  name: &str,
  func: sys::JSCFunction,
  length: c_int,
) {
  let c = CString::new(name).unwrap();
  let f = sys::JS_NewCFunction2(ctx, func, c.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
  sys::JS_SetPropertyStr(ctx, obj, c.as_ptr(), f);
}

unsafe fn define_readonly(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &str, getter: Getter) {
  let cname = CString::new(name).unwrap();
  let f = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<Getter, sys::JSCFunction>(getter),
    cname.as_ptr(),
    0,
    sys::JS_CFUNC_GETTER,
    0,
  );
  let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
  sys::JS_DefinePropertyGetSet(
    ctx,
    obj,
    atom,
    f,
    sys::js_undefined(),
    sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
  );
  sys::JS_FreeAtom(ctx, atom);
}

unsafe fn define_readwrite(
  ctx: *mut sys::JSContext,
  obj: sys::JSValue,
  name: &str,
  getter: Getter,
  setter: Setter,
) {
  let cname = CString::new(name).unwrap();
  let g = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<Getter, sys::JSCFunction>(getter),
    cname.as_ptr(),
    0,
    sys::JS_CFUNC_GETTER,
    0,
  );
  let s = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<Setter, sys::JSCFunction>(setter),
    cname.as_ptr(),
    1,
    sys::JS_CFUNC_SETTER,
    0,
  );
  let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
  sys::JS_DefinePropertyGetSet(
    ctx,
    obj,
    atom,
    g,
    s,
    sys::JS_PROP_HAS_GET
      | sys::JS_PROP_HAS_SET
      | sys::JS_PROP_CONFIGURABLE
      | sys::JS_PROP_ENUMERABLE,
  );
  sys::JS_FreeAtom(ctx, atom);
}

/// Build a TypeError and throw it via JS_Throw.
unsafe fn type_error(ctx: *mut sys::JSContext, msg: &str) -> sys::JSValue {
  let msg_val = js_string(ctx, msg);
  let name = CString::new("TypeError").unwrap();
  let global = sys::JS_GetGlobalObject(ctx);
  let error_ctor = sys::JS_GetPropertyStr(ctx, global, name.as_ptr());
  let mut arg = msg_val;
  let error = sys::JS_Call(ctx, error_ctor, sys::js_undefined(), 1, &mut arg);
  sys::JS_FreeValue(ctx, msg_val);
  sys::JS_FreeValue(ctx, error_ctor);
  sys::JS_FreeValue(ctx, global);
  sys::JS_Throw(ctx, error);
  sys::js_exception()
}

// ---------------------------------------------------------------------------
// URLSearchParams
// ---------------------------------------------------------------------------

pub(crate) const URL_SEARCH_PARAMS_CLASS_KIND: &str = "URLSearchParams";

struct UrlSearchParamsInner {
  pairs: Vec<(String, String)>,
}

unsafe fn sp_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut UrlSearchParamsInner {
  let class_id = crate::class_registry::class_id_for(rt, URL_SEARCH_PARAMS_CLASS_KIND);
  sys::JS_GetOpaque(this_val, class_id) as *mut UrlSearchParamsInner
}

unsafe extern "C" fn sp_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
  let ptr = sp_opaque(rt, val);
  if !ptr.is_null() {
    drop(Box::from_raw(ptr));
  }
}

unsafe fn parse_sp_init(ctx: *mut sys::JSContext, init: sys::JSValue) -> Vec<(String, String)> {
  let mut pairs = Vec::new();
  if init.tag == sys::JS_TAG_STRING {
    if let Some(s) = read_js_string(ctx, init) {
      if s.is_empty() {
        return pairs;
      }
      for part in s.split('&') {
        let (k, v) = match part.find('=') {
          Some(pos) => (
            urlencoding::decode(&part[..pos])
              .unwrap_or_default()
              .into_owned(),
            urlencoding::decode(&part[pos + 1..])
              .unwrap_or_default()
              .into_owned(),
          ),
          None => (
            urlencoding::decode(part).unwrap_or_default().into_owned(),
            String::new(),
          ),
        };
        pairs.push((k, v));
      }
    }
    return pairs;
  }
  if sys::JS_IsArray(init) {
    let mut len: i64 = 0;
    sys::JS_GetLength(ctx, init, &mut len);
    for i in 0..(len as u32) {
      let pair = sys::JS_GetPropertyUint32(ctx, init, i);
      if sys::JS_IsArray(pair) {
        let mut plen: i64 = 0;
        sys::JS_GetLength(ctx, pair, &mut plen);
        if plen >= 2 {
          let k = sys::JS_GetPropertyUint32(ctx, pair, 0);
          let v = sys::JS_GetPropertyUint32(ctx, pair, 1);
          let key = read_js_string(ctx, k).unwrap_or_default();
          let val = read_js_string(ctx, v).unwrap_or_default();
          sys::JS_FreeValue(ctx, k);
          sys::JS_FreeValue(ctx, v);
          pairs.push((key, val));
        }
      }
      sys::JS_FreeValue(ctx, pair);
    }
  }
  pairs
}

unsafe fn build_search_string(pairs: &[(String, String)]) -> String {
  if pairs.is_empty() {
    return String::new();
  }
  let mut parts = Vec::with_capacity(pairs.len());
  for (k, v) in pairs {
    parts.push(format!(
      "{}={}",
      urlencoding::encode(k),
      urlencoding::encode(v)
    ));
  }
  parts.join("&")
}

/// Helper: get the `_sp` URLSearchParams object attached to a URL instance
/// and return a clone of its pairs. Caller must free `sp_val`.
unsafe fn get_sp_pairs_for_url(
  ctx: *mut sys::JSContext,
  url_this: sys::JSValue,
) -> (sys::JSValue, Option<Vec<(String, String)>>) {
  let rt = sys::JS_GetRuntime(ctx);
  let sp_val = get_property(ctx, url_this, "_sp");
  if sp_val.tag == sys::JS_TAG_OBJECT {
    let sp_inner = sp_opaque(rt, sp_val);
    if !sp_inner.is_null() {
      return (sp_val, Some((*sp_inner).pairs.clone()));
    }
  }
  (sp_val, None)
}

/// Helper: overwrite the `_sp` URLSearchParams's pairs from a parsed query.
unsafe fn set_sp_pairs_for_url(
  ctx: *mut sys::JSContext,
  url_this: sys::JSValue,
  pairs: Vec<(String, String)>,
) {
  let rt = sys::JS_GetRuntime(ctx);
  let sp_val = get_property(ctx, url_this, "_sp");
  if sp_val.tag == sys::JS_TAG_OBJECT {
    let sp_inner = sp_opaque(rt, sp_val);
    if !sp_inner.is_null() {
      (*sp_inner).pairs = pairs;
    }
  }
  sys::JS_FreeValue(ctx, sp_val);
}

// -- URLSearchParams JS methods --

unsafe extern "C" fn sp_append(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  if inner.is_null() || argc < 2 {
    return sys::js_undefined();
  }
  let key = read_js_string(ctx, *argv).unwrap_or_default();
  let val = read_js_string(ctx, *argv.add(1)).unwrap_or_default();
  (*inner).pairs.push((key, val));
  sys::js_undefined()
}

unsafe extern "C" fn sp_delete(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  if inner.is_null() || argc < 1 {
    return sys::js_undefined();
  }
  let key = read_js_string(ctx, *argv).unwrap_or_default();
  (*inner).pairs.retain(|(k, _)| k != &key);
  sys::js_undefined()
}

unsafe extern "C" fn sp_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  if inner.is_null() || argc < 1 {
    return sys::js_null();
  }
  let key = read_js_string(ctx, *argv).unwrap_or_default();
  match (*inner).pairs.iter().find(|(k, _)| k == &key) {
    Some((_, v)) => js_string(ctx, v),
    None => sys::js_null(),
  }
}

unsafe extern "C" fn sp_get_all(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  let arr = sys::JS_NewArray(ctx);
  if inner.is_null() || argc < 1 {
    return arr;
  }
  let key = read_js_string(ctx, *argv).unwrap_or_default();
  let mut i = 0u32;
  for (_, v) in (*inner).pairs.iter().filter(|(k, _)| k == &key) {
    let v_val = js_string(ctx, v);
    sys::JS_SetPropertyUint32(ctx, arr, i, v_val);
    i += 1;
  }
  arr
}

unsafe extern "C" fn sp_has(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  if inner.is_null() || argc < 1 {
    return sys::js_bool(false);
  }
  let key = read_js_string(ctx, *argv).unwrap_or_default();
  sys::js_bool((*inner).pairs.iter().any(|(k, _)| k == &key))
}

unsafe extern "C" fn sp_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  if inner.is_null() || argc < 2 {
    return sys::js_undefined();
  }
  let key = read_js_string(ctx, *argv).unwrap_or_default();
  let val = read_js_string(ctx, *argv.add(1)).unwrap_or_default();
  (*inner).pairs.retain(|(k, _)| k != &key);
  (*inner).pairs.push((key, val));
  sys::js_undefined()
}

unsafe extern "C" fn sp_sort(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  if !inner.is_null() {
    (*inner).pairs.sort_by(|a, b| a.0.cmp(&b.0));
  }
  sys::js_undefined()
}

unsafe extern "C" fn sp_to_string(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  js_string(ctx, &build_search_string(&(*inner).pairs))
}

unsafe extern "C" fn sp_entries(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  let arr = sys::JS_NewArray(ctx);
  if inner.is_null() {
    return arr;
  }
  for (i, (k, v)) in (*inner).pairs.iter().enumerate() {
    let pair = sys::JS_NewArray(ctx);
    let k_val = js_string(ctx, k);
    let v_val = js_string(ctx, v);
    sys::JS_SetPropertyUint32(ctx, pair, 0, k_val);
    sys::JS_SetPropertyUint32(ctx, pair, 1, v_val);
    sys::JS_SetPropertyUint32(ctx, arr, i as u32, pair);
  }
  arr
}

unsafe extern "C" fn sp_keys(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  let arr = sys::JS_NewArray(ctx);
  if inner.is_null() {
    return arr;
  }
  for (i, (k, _)) in (*inner).pairs.iter().enumerate() {
    let k_val = js_string(ctx, k);
    sys::JS_SetPropertyUint32(ctx, arr, i as u32, k_val);
  }
  arr
}

unsafe extern "C" fn sp_values(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  let arr = sys::JS_NewArray(ctx);
  if inner.is_null() {
    return arr;
  }
  for (i, (_, v)) in (*inner).pairs.iter().enumerate() {
    let v_val = js_string(ctx, v);
    sys::JS_SetPropertyUint32(ctx, arr, i as u32, v_val);
  }
  arr
}

unsafe extern "C" fn sp_for_each(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  if inner.is_null() || argc < 1 {
    return sys::js_undefined();
  }
  let callback = *argv;
  for (k, v) in &(*inner).pairs {
    let mut args = [js_string(ctx, v), js_string(ctx, k), sys::js_undefined()];
    let result = sys::JS_Call(ctx, callback, this_val, 3, args.as_mut_ptr());
    sys::JS_FreeValue(ctx, result);
    sys::JS_FreeValue(ctx, args[0]);
    sys::JS_FreeValue(ctx, args[1]);
  }
  sys::js_undefined()
}

unsafe extern "C" fn sp_length_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = sp_opaque(rt, this_val);
  let len = if inner.is_null() {
    0
  } else {
    (*inner).pairs.len()
  };
  sys::js_float64(len as f64)
}

unsafe extern "C" fn sp_constructor(
  ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let class_id = crate::class_registry::ensure_class(
    rt,
    URL_SEARCH_PARAMS_CLASS_KIND,
    &sys::JSClassDef {
      class_name: b"URLSearchParams\0".as_ptr().cast(),
      finalizer: Some(sp_finalizer),
      gc_mark: std::ptr::null_mut(),
      call: std::ptr::null_mut(),
      exotic: std::ptr::null_mut(),
    },
  );
  let init = if argc >= 1 {
    *argv
  } else {
    sys::js_undefined()
  };
  let pairs = parse_sp_init(ctx, init);
  let inner = Box::new(UrlSearchParamsInner { pairs });
  let obj = sys::JS_NewObjectClass(ctx, class_id);
  sys::JS_SetOpaque(obj, Box::into_raw(inner) as *mut std::os::raw::c_void);
  obj
}

/// Registers `URLSearchParams` class and creates a prototype with methods.
unsafe fn register_url_search_params(ctx: *mut sys::JSContext) {
  let rt = sys::JS_GetRuntime(ctx);
  let class_id = crate::class_registry::ensure_class(
    rt,
    URL_SEARCH_PARAMS_CLASS_KIND,
    &sys::JSClassDef {
      class_name: b"URLSearchParams\0".as_ptr().cast(),
      finalizer: Some(sp_finalizer),
      gc_mark: std::ptr::null_mut(),
      call: std::ptr::null_mut(),
      exotic: std::ptr::null_mut(),
    },
  );

  let proto = sys::JS_NewObject(ctx);
  define_method(ctx, proto, "append", sp_append, 2);
  define_method(ctx, proto, "delete", sp_delete, 1);
  define_method(ctx, proto, "get", sp_get, 1);
  define_method(ctx, proto, "getAll", sp_get_all, 1);
  define_method(ctx, proto, "has", sp_has, 1);
  define_method(ctx, proto, "set", sp_set, 2);
  define_method(ctx, proto, "sort", sp_sort, 0);
  define_method(ctx, proto, "toString", sp_to_string, 0);
  define_method(ctx, proto, "entries", sp_entries, 0);
  define_method(ctx, proto, "keys", sp_keys, 0);
  define_method(ctx, proto, "values", sp_values, 0);
  define_method(ctx, proto, "forEach", sp_for_each, 1);
  define_readonly(ctx, proto, "length", sp_length_get);
  sys::JS_SetClassProto(ctx, class_id, proto);

  let global = sys::JS_GetGlobalObject(ctx);
  let ctor = sys::JS_NewCFunction2(
    ctx,
    sp_constructor,
    b"URLSearchParams\0".as_ptr().cast(),
    1,
    sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
    0,
  );
  set_property(ctx, global, "URLSearchParams", ctor);
  sys::JS_FreeValue(ctx, global);
}

// ---------------------------------------------------------------------------
// URL
// ---------------------------------------------------------------------------

pub(crate) const URL_CLASS_KIND: &str = "URL";

struct UrlInner {
  url: url::Url,
}

unsafe fn url_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut UrlInner {
  let class_id = crate::class_registry::class_id_for(rt, URL_CLASS_KIND);
  sys::JS_GetOpaque(this_val, class_id) as *mut UrlInner
}

unsafe extern "C" fn url_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
  let ptr = url_opaque(rt, val);
  if !ptr.is_null() {
    drop(Box::from_raw(ptr));
  }
}

unsafe fn url_display_from_sp(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  let (sp_val, pairs_opt) = get_sp_pairs_for_url(ctx, this_val);
  let pairs_ref = pairs_opt.as_ref();
  let mut url = (*inner).url.clone();
  if let Some(pairs) = pairs_ref {
    let search = build_search_string(pairs);
    if search.is_empty() {
      url.set_query(None);
    } else {
      url.set_query(Some(&search));
    }
  }
  sys::JS_FreeValue(ctx, sp_val);
  js_string(ctx, &url.to_string())
}

// -- URL JS getters/setters --

unsafe extern "C" fn url_href_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  url_display_from_sp(ctx, this_val)
}

unsafe extern "C" fn url_href_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return sys::js_undefined();
  }
  if let Some(s) = read_js_string(ctx, val) {
    if let Ok(new_url) = url::Url::parse(&s) {
      let new_pairs: Vec<(String, String)> = new_url
        .query_pairs()
        .into_owned()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
      set_sp_pairs_for_url(ctx, this_val, new_pairs);
      (*inner).url = new_url;
    }
  }
  sys::js_undefined()
}

unsafe extern "C" fn url_origin_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "null");
  }
  js_string(ctx, &(*inner).url.origin().ascii_serialization())
}

unsafe extern "C" fn url_protocol_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  js_string(ctx, &format!("{}:", (*inner).url.scheme()))
}

unsafe extern "C" fn url_protocol_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if !inner.is_null() {
    if let Some(s) = read_js_string(ctx, val) {
      let scheme = s.trim_end_matches(':');
      let _ = (*inner).url.set_scheme(scheme);
    }
  }
  sys::js_undefined()
}

unsafe extern "C" fn url_username_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  js_string(ctx, (*inner).url.username())
}

unsafe extern "C" fn url_username_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if !inner.is_null() {
    if let Some(s) = read_js_string(ctx, val) {
      let _ = (*inner).url.set_username(&s);
    }
  }
  sys::js_undefined()
}

unsafe extern "C" fn url_password_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  js_string(ctx, (*inner).url.password().unwrap_or(""))
}

unsafe extern "C" fn url_password_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if !inner.is_null() {
    if let Some(s) = read_js_string(ctx, val) {
      let _ = (*inner).url.set_password(Some(&s));
    }
  }
  sys::js_undefined()
}

unsafe extern "C" fn url_host_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  match ((*inner).url.host_str(), (*inner).url.port()) {
    (Some(host), Some(port)) => js_string(ctx, &format!("{host}:{port}")),
    (Some(host), None) => js_string(ctx, host),
    (None, _) => js_string(ctx, ""),
  }
}

unsafe extern "C" fn url_host_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if !inner.is_null() {
    if let Some(s) = read_js_string(ctx, val) {
      let _ = (*inner).url.set_host(Some(&s));
    }
  }
  sys::js_undefined()
}

unsafe extern "C" fn url_hostname_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  js_string(ctx, (*inner).url.host_str().unwrap_or(""))
}

unsafe extern "C" fn url_hostname_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if !inner.is_null() {
    if let Some(s) = read_js_string(ctx, val) {
      let _ = (*inner).url.set_host(Some(&s));
    }
  }
  sys::js_undefined()
}

unsafe extern "C" fn url_port_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  js_string(
    ctx,
    &(*inner).url.port().map_or(String::new(), |p| p.to_string()),
  )
}

unsafe extern "C" fn url_port_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if !inner.is_null() {
    if let Some(s) = read_js_string(ctx, val) {
      if s.is_empty() {
        let _ = (*inner).url.set_port(None);
      } else if let Ok(port) = s.parse::<u16>() {
        let _ = (*inner).url.set_port(Some(port));
      }
    }
  }
  sys::js_undefined()
}

unsafe extern "C" fn url_pathname_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  js_string(ctx, (*inner).url.path())
}

unsafe extern "C" fn url_pathname_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if !inner.is_null() {
    if let Some(s) = read_js_string(ctx, val) {
      let _ = (*inner).url.set_path(&s);
    }
  }
  sys::js_undefined()
}

unsafe extern "C" fn url_search_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  let (sp_val, pairs_opt) = get_sp_pairs_for_url(ctx, this_val);
  if let Some(pairs) = &pairs_opt {
    let search = build_search_string(pairs);
    sys::JS_FreeValue(ctx, sp_val);
    if search.is_empty() {
      return js_string(ctx, "");
    }
    return js_string(ctx, &format!("?{search}"));
  }
  sys::JS_FreeValue(ctx, sp_val);
  js_string(ctx, "")
}

unsafe extern "C" fn url_search_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue {
  let new_pairs = if let Some(s) = read_js_string(ctx, val) {
    let s = s.trim_start_matches('?');
    if s.is_empty() {
      Vec::new()
    } else {
      s.split('&')
        .filter_map(|part| {
          let (k, v) = match part.find('=') {
            Some(pos) => (
              urlencoding::decode(&part[..pos])
                .unwrap_or_default()
                .into_owned(),
              urlencoding::decode(&part[pos + 1..])
                .unwrap_or_default()
                .into_owned(),
            ),
            None => (
              urlencoding::decode(part).unwrap_or_default().into_owned(),
              String::new(),
            ),
          };
          if k.is_empty() {
            None
          } else {
            Some((k, v))
          }
        })
        .collect()
    }
  } else {
    Vec::new()
  };
  set_sp_pairs_for_url(ctx, this_val, new_pairs);
  sys::js_undefined()
}

unsafe extern "C" fn url_search_params_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  get_property(ctx, this_val, "_sp")
}

unsafe extern "C" fn url_hash_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if inner.is_null() {
    return js_string(ctx, "");
  }
  match (*inner).url.fragment() {
    Some(f) => js_string(ctx, &format!("#{f}")),
    None => js_string(ctx, ""),
  }
}

unsafe extern "C" fn url_hash_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue {
  let rt = sys::JS_GetRuntime(ctx);
  let inner = url_opaque(rt, this_val);
  if !inner.is_null() {
    if let Some(s) = read_js_string(ctx, val) {
      let s = s.trim_start_matches('#');
      if s.is_empty() {
        (*inner).url.set_fragment(None);
      } else {
        (*inner).url.set_fragment(Some(s));
      }
    }
  }
  sys::js_undefined()
}

unsafe extern "C" fn url_to_string_fn(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  url_href_get(ctx, this_val)
}

unsafe extern "C" fn url_to_json(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  url_to_string_fn(ctx, this_val, argc, argv)
}

// -- URL constructor --

unsafe extern "C" fn url_constructor(
  ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return type_error(ctx, "URL: 1 argument required, but only 0 present.");
  }
  let url_str = match read_js_string(ctx, *argv) {
    Some(s) => s,
    None => return type_error(ctx, "URL: invalid URL"),
  };
  let parsed = if argc >= 2 {
    let base_str = read_js_string(ctx, *argv.add(1)).unwrap_or_default();
    match url::Url::parse(&base_str) {
      Ok(base) => match base.join(&url_str) {
        Ok(u) => u,
        Err(_) => return type_error(ctx, &format!("URL: invalid URL '{url_str}'")),
      },
      Err(_) => return type_error(ctx, &format!("URL: invalid base URL '{base_str}'")),
    }
  } else {
    match url::Url::parse(&url_str) {
      Ok(u) => u,
      Err(_) => return type_error(ctx, &format!("URL: invalid URL '{url_str}'")),
    }
  };

  let search_pairs: Vec<(String, String)> = parsed
    .query_pairs()
    .into_owned()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();

  let rt = sys::JS_GetRuntime(ctx);
  let url_class_id = crate::class_registry::ensure_class(
    rt,
    URL_CLASS_KIND,
    &sys::JSClassDef {
      class_name: b"URL\0".as_ptr().cast(),
      finalizer: Some(url_finalizer),
      gc_mark: std::ptr::null_mut(),
      call: std::ptr::null_mut(),
      exotic: std::ptr::null_mut(),
    },
  );

  let obj = sys::JS_NewObjectClass(ctx, url_class_id);
  let inner = Box::new(UrlInner { url: parsed });
  sys::JS_SetOpaque(obj, Box::into_raw(inner) as *mut std::os::raw::c_void);

  let sp_class_id = crate::class_registry::ensure_class(
    rt,
    URL_SEARCH_PARAMS_CLASS_KIND,
    &sys::JSClassDef {
      class_name: b"URLSearchParams\0".as_ptr().cast(),
      finalizer: Some(sp_finalizer),
      gc_mark: std::ptr::null_mut(),
      call: std::ptr::null_mut(),
      exotic: std::ptr::null_mut(),
    },
  );
  let sp_inner = Box::new(UrlSearchParamsInner {
    pairs: search_pairs,
  });
  let sp_obj = sys::JS_NewObjectClass(ctx, sp_class_id);
  sys::JS_SetOpaque(sp_obj, Box::into_raw(sp_inner) as *mut std::os::raw::c_void);

  let sp_name = CString::new("_sp").unwrap();
  sys::JS_SetPropertyStr(ctx, obj, sp_name.as_ptr(), sp_obj);

  obj
}

/// Registers `URL` class with a prototype containing getters/setters.
unsafe fn register_url(ctx: *mut sys::JSContext) {
  let rt = sys::JS_GetRuntime(ctx);
  let class_id = crate::class_registry::ensure_class(
    rt,
    URL_CLASS_KIND,
    &sys::JSClassDef {
      class_name: b"URL\0".as_ptr().cast(),
      finalizer: Some(url_finalizer),
      gc_mark: std::ptr::null_mut(),
      call: std::ptr::null_mut(),
      exotic: std::ptr::null_mut(),
    },
  );

  let proto = sys::JS_NewObject(ctx);
  define_readwrite(ctx, proto, "href", url_href_get, url_href_set);
  define_readonly(ctx, proto, "origin", url_origin_get);
  define_readwrite(ctx, proto, "protocol", url_protocol_get, url_protocol_set);
  define_readwrite(ctx, proto, "username", url_username_get, url_username_set);
  define_readwrite(ctx, proto, "password", url_password_get, url_password_set);
  define_readwrite(ctx, proto, "host", url_host_get, url_host_set);
  define_readwrite(ctx, proto, "hostname", url_hostname_get, url_hostname_set);
  define_readwrite(ctx, proto, "port", url_port_get, url_port_set);
  define_readwrite(ctx, proto, "pathname", url_pathname_get, url_pathname_set);
  define_readwrite(ctx, proto, "search", url_search_get, url_search_set);
  define_readonly(ctx, proto, "searchParams", url_search_params_get);
  define_readwrite(ctx, proto, "hash", url_hash_get, url_hash_set);
  define_method(ctx, proto, "toString", url_to_string_fn, 0);
  define_method(ctx, proto, "toJSON", url_to_json, 0);
  sys::JS_SetClassProto(ctx, class_id, proto);

  let global = sys::JS_GetGlobalObject(ctx);
  let ctor = sys::JS_NewCFunction2(
    ctx,
    url_constructor,
    b"URL\0".as_ptr().cast(),
    1,
    sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
    0,
  );

  let existing_url = get_property(ctx, global, "URL");
  if existing_url.tag == sys::JS_TAG_OBJECT {
    let create_object_url = get_property(ctx, existing_url, "createObjectURL");
    if create_object_url.tag != sys::JS_TAG_UNDEFINED {
      set_property(ctx, ctor, "createObjectURL", create_object_url);
    }
    let revoke_object_url = get_property(ctx, existing_url, "revokeObjectURL");
    if revoke_object_url.tag != sys::JS_TAG_UNDEFINED {
      set_property(ctx, ctor, "revokeObjectURL", revoke_object_url);
    }
    sys::JS_FreeValue(ctx, existing_url);
  } else {
    sys::JS_FreeValue(ctx, existing_url);
  }

  set_property(ctx, global, "URL", ctor);
  sys::JS_FreeValue(ctx, global);
}

// ---------------------------------------------------------------------------
// Public registration
// ---------------------------------------------------------------------------

/// Registers `URL`, `URLSearchParams` as globals.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
  register_url_search_params(ctx);
  register_url(ctx);
}
