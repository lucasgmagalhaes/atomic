//! Small shared string/error helpers and the class-kind name used by three
//! or more `form_data` submodules (mirrors `dom_bindings/util.rs`'s
//! convention: constants/helpers used by only one submodule stay local to
//! that submodule instead of living here).

use quickjs_sys as sys;

/// The `class_registry` key this module's `FormData` class is stored
/// under — shared by [`super::encoding`] (`serialize`), [`super::class`]
/// (opaque-data access, the constructor) and [`super::register`].
pub(super) const FORM_DATA_CLASS_KIND: &str = "FormData";

pub(super) unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
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

pub(super) unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
  sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

pub(super) unsafe fn throw_type_error(ctx: *mut sys::JSContext, message: &str) -> sys::JSValue {
  sys::JS_Throw(ctx, new_js_string(ctx, message))
}
