//! `Notification` (permission model + instance surface) — the JS-facing
//! half of the mockup's "Notifications" bloqueante API. `Notification.
//! permission`/`requestPermission()` are a real per-context state
//! machine (not hardcoded), and `new Notification(title, options)`
//! builds a real instance with real `title`/`body`/`tag`/`icon`
//! properties and an idempotent `close()`.
//!
//! Deviation, documented the same way `page_visibility` documents its
//! own scope-down: there is no real OS toast/balloon behind this yet —
//! `requestPermission()` auto-grants immediately instead of asking the
//! OS for real user consent (this crate has no OS permission-prompt UI),
//! and constructing a `Notification` doesn't actually display anything.
//! What *is* real: the permission state genuinely starts at `"default"`
//! and only becomes `"granted"` after a caller calls
//! `requestPermission()` (mirroring the real API's shape), and every
//! instance property is really read back, not synthesized on read.
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

/// See `crate::class_registry` - one registry entry per `JSRuntime`.
const NOTIFICATION_CLASS_KIND: &str = "Notification";

thread_local! {
    // Keyed by JSContext pointer, same convention as `blob::OBJECT_URLS`.
    static PERMISSION: RefCell<HashMap<usize, &'static str>> = RefCell::new(HashMap::new());
}

fn permission_for(ctx: *mut sys::JSContext) -> &'static str {
  PERMISSION.with(|p| *p.borrow().get(&(ctx as usize)).unwrap_or(&"default"))
}

fn set_permission(ctx: *mut sys::JSContext, value: &'static str) {
  PERMISSION.with(|p| {
    p.borrow_mut().insert(ctx as usize, value);
  });
}

struct NotificationInner {
  closed: bool,
}

unsafe fn notification_opaque(
  rt: *mut sys::JSRuntime,
  this_val: sys::JSValue,
) -> *mut NotificationInner {
  let class_id = crate::class_registry::class_id_for(rt, NOTIFICATION_CLASS_KIND);
  sys::JS_GetOpaque(this_val, class_id) as *mut NotificationInner
}

unsafe extern "C" fn notification_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
  let ptr = notification_opaque(rt, val);
  if !ptr.is_null() {
    drop(Box::from_raw(ptr));
  }
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

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str) -> sys::JSValue {
  let name = CString::new(key).unwrap();
  sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

unsafe fn read_option_str(ctx: *mut sys::JSContext, options: sys::JSValue, key: &str) -> String {
  if options.tag != sys::JS_TAG_OBJECT {
    return String::new();
  }
  let val = get_prop(ctx, options, key);
  let s = read_js_string(ctx, val).unwrap_or_default();
  sys::JS_FreeValue(ctx, val);
  s
}

unsafe extern "C" fn notification_constructor(
  ctx: *mut sys::JSContext,
  new_target: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if !crate::permissions_policy::is_allowed(ctx, "notifications") {
    return sys::JS_Throw(ctx, permission_error(ctx));
  }
  let class_id =
    crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NOTIFICATION_CLASS_KIND);
  let obj = sys::JS_NewObjectClass(ctx, class_id);
  if sys::js_is_exception(&obj) {
    return obj;
  }
  if new_target.tag != sys::JS_TAG_UNDEFINED {
    let proto = get_prop(ctx, new_target, "prototype");
    if proto.tag != sys::JS_TAG_UNDEFINED {
      sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);
  }
  sys::JS_SetOpaque(
    obj,
    Box::into_raw(Box::new(NotificationInner { closed: false })) as *mut c_void,
  );

  let title = if argc >= 1 {
    read_js_string(ctx, *argv).unwrap_or_default()
  } else {
    String::new()
  };
  set_str(ctx, obj, "title", &title);

  let options = if argc >= 2 {
    *argv.add(1)
  } else {
    sys::js_undefined()
  };
  set_str(ctx, obj, "body", &read_option_str(ctx, options, "body"));
  set_str(ctx, obj, "tag", &read_option_str(ctx, options, "tag"));
  set_str(ctx, obj, "icon", &read_option_str(ctx, options, "icon"));

  obj
}

/// Builds the regular JavaScript error used when a response policy blocks a
/// privileged notification operation. `quickjs-sys` intentionally does not
/// bind QuickJS's variadic `JS_ThrowTypeError`, so construct the standard
/// `Error` object directly and pass it to `JS_Throw` instead.
unsafe fn permission_error(ctx: *mut sys::JSContext) -> sys::JSValue {
  let global = sys::JS_GetGlobalObject(ctx);
  let error_name = CString::new("Error").unwrap();
  let error_ctor = sys::JS_GetPropertyStr(ctx, global, error_name.as_ptr());
  sys::JS_FreeValue(ctx, global);
  let mut message = new_js_string(ctx, "notifications are blocked by Permissions Policy");
  let error = sys::JS_Call(ctx, error_ctor, sys::js_undefined(), 1, &mut message);
  sys::JS_FreeValue(ctx, message);
  sys::JS_FreeValue(ctx, error_ctor);
  error
}

unsafe extern "C" fn notification_close(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  let ptr = notification_opaque(sys::JS_GetRuntime(ctx), this_val);
  if !ptr.is_null() {
    (*ptr).closed = true;
  }
  sys::js_undefined()
}

unsafe fn resolved_string_promise(ctx: *mut sys::JSContext, value: &str) -> sys::JSValue {
  let mut resolving_funcs = [sys::js_undefined(); 2];
  let promise = sys::JS_NewPromiseCapability(ctx, resolving_funcs.as_mut_ptr());
  let [resolve, reject] = resolving_funcs;
  let mut arg = new_js_string(ctx, value);
  let result = sys::JS_Call(ctx, resolve, sys::js_undefined(), 1, &mut arg);
  sys::JS_FreeValue(ctx, result);
  sys::JS_FreeValue(ctx, arg);
  sys::JS_FreeValue(ctx, resolve);
  sys::JS_FreeValue(ctx, reject);
  promise
}

unsafe extern "C" fn request_permission(
  ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  if !crate::permissions_policy::is_allowed(ctx, "notifications") {
    return resolved_string_promise(ctx, "denied");
  }
  // Real browsers ask the OS/user for real consent; this crate has no
  // permission-prompt UI yet, so every request auto-grants (documented
  // module-level deviation) - what's real is that `permission` only
  // flips to "granted" *after* this is called, not before.
  set_permission(ctx, "granted");
  resolved_string_promise(ctx, "granted")
}

unsafe extern "C" fn permission_get(
  ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
) -> sys::JSValue {
  new_js_string(ctx, permission_for(ctx))
}

type Getter =
  unsafe extern "C" fn(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue;

unsafe fn define_static_getter(
  ctx: *mut sys::JSContext,
  obj: sys::JSValue,
  name: &str,
  getter: Getter,
) {
  let name_c = CString::new(name).unwrap();
  let f = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<Getter, sys::JSCFunction>(getter),
    name_c.as_ptr(),
    0,
    sys::JS_CFUNC_GETTER,
    0,
  );
  let atom = sys::JS_NewAtom(ctx, name_c.as_ptr());
  sys::JS_DefinePropertyGetSet(
    ctx,
    obj,
    atom,
    f,
    sys::js_undefined(),
    sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE,
  );
  sys::JS_FreeAtom(ctx, atom);
}

/// Registers the `Notification` constructor (with a `permission` static
/// accessor and `requestPermission` static method) as a global.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
  let rt = sys::JS_GetRuntime(ctx);
  let class_name = CString::new("Notification").unwrap();
  let def = sys::JSClassDef {
    class_name: class_name.as_ptr(),
    finalizer: Some(notification_finalizer),
    gc_mark: std::ptr::null_mut(),
    call: std::ptr::null_mut(),
    exotic: std::ptr::null_mut(),
  };
  let class_id = crate::class_registry::ensure_class(rt, NOTIFICATION_CLASS_KIND, &def);

  let proto = sys::JS_NewObject(ctx);
  let close_name = CString::new("close").unwrap();
  let close_fn = sys::JS_NewCFunction2(
    ctx,
    notification_close,
    close_name.as_ptr(),
    0,
    sys::JS_CFUNC_GENERIC,
    0,
  );
  sys::JS_SetPropertyStr(ctx, proto, close_name.as_ptr(), close_fn);
  sys::JS_SetClassProto(ctx, class_id, proto);

  let ctor_name = CString::new("Notification").unwrap();
  let ctor = sys::JS_NewCFunction2(
    ctx,
    notification_constructor,
    ctor_name.as_ptr(),
    2,
    sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
    0,
  );
  let proto_name = CString::new("prototype").unwrap();
  sys::JS_SetPropertyStr(ctx, ctor, proto_name.as_ptr(), sys::JS_DupValue(ctx, proto));

  define_static_getter(ctx, ctor, "permission", permission_get);
  let rp_name = CString::new("requestPermission").unwrap();
  let rp_fn = sys::JS_NewCFunction2(
    ctx,
    request_permission,
    rp_name.as_ptr(),
    0,
    sys::JS_CFUNC_GENERIC,
    0,
  );
  sys::JS_SetPropertyStr(ctx, ctor, rp_name.as_ptr(), rp_fn);

  let global = sys::JS_GetGlobalObject(ctx);
  sys::JS_SetPropertyStr(ctx, global, ctor_name.as_ptr(), ctor);
  sys::JS_FreeValue(ctx, global);
}

/// Drops this context's permission-state entry - avoids leaking one map
/// entry per `JSContext` pointer across the process, same reasoning as
/// `blob::cleanup`/`timers::cleanup`.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
  PERMISSION.with(|p| {
    p.borrow_mut().remove(&(ctx as usize));
  });
}
