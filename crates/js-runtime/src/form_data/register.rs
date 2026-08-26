//! Wires the `FormData` class + prototype methods + constructor into the
//! global object.

use std::ffi::CString;

use quickjs_sys as sys;

use super::class::form_data_constructor;
use super::methods::{
  form_data_append, form_data_delete, form_data_entries, form_data_for_each, form_data_get,
  form_data_get_all, form_data_has, form_data_keys, form_data_set, form_data_values,
};
use super::types::form_data_finalizer;
use super::util::FORM_DATA_CLASS_KIND;

/// Registers the `FormData` global. Safe to call without a DOM behind the
/// context — the `new FormData(form)` population path degrades to an empty
/// instance there (see module docs).
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
  let rt = sys::JS_GetRuntime(ctx);
  let class_name = CString::new("FormData").unwrap();
  let def = sys::JSClassDef {
    class_name: class_name.as_ptr(),
    finalizer: Some(form_data_finalizer),
    gc_mark: std::ptr::null_mut(),
    call: std::ptr::null_mut(),
    exotic: std::ptr::null_mut(),
  };
  let class_id = crate::class_registry::ensure_class(rt, FORM_DATA_CLASS_KIND, &def);

  let proto = sys::JS_NewObject(ctx);
  for (name, func, length) in [
    ("append", form_data_append as sys::JSCFunction, 2),
    ("set", form_data_set as sys::JSCFunction, 2),
    ("get", form_data_get as sys::JSCFunction, 1),
    ("getAll", form_data_get_all as sys::JSCFunction, 1),
    ("has", form_data_has as sys::JSCFunction, 1),
    ("delete", form_data_delete as sys::JSCFunction, 1),
    ("entries", form_data_entries as sys::JSCFunction, 0),
    ("keys", form_data_keys as sys::JSCFunction, 0),
    ("values", form_data_values as sys::JSCFunction, 0),
    ("forEach", form_data_for_each as sys::JSCFunction, 1),
  ] {
    let cname = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, proto, cname.as_ptr(), f);
  }

  let ctor_name = CString::new("FormData").unwrap();
  let ctor_fn = sys::JS_NewCFunction2(
    ctx,
    form_data_constructor,
    ctor_name.as_ptr(),
    1,
    sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
    0,
  );
  let proto_name = CString::new("prototype").unwrap();
  sys::JS_SetPropertyStr(
    ctx,
    ctor_fn,
    proto_name.as_ptr(),
    sys::JS_DupValue(ctx, proto),
  );
  sys::JS_SetClassProto(ctx, class_id, proto);

  let global = sys::JS_GetGlobalObject(ctx);
  sys::JS_SetPropertyStr(ctx, global, ctor_name.as_ptr(), ctor_fn);
  sys::JS_FreeValue(ctx, global);
}
