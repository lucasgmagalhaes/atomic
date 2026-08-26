//! Small shared string/error helpers and length-bound constants used by
//! three or more `dom_bindings` submodules. Constants used by only one
//! submodule stay local to that submodule instead of living here.

use quickjs_sys as sys;

/// Bounds a `setAttribute`/`classList` token name (`attributes.rs`,
/// `class_list.rs`).
pub(super) const MAX_ATTRIBUTE_NAME_LENGTH: usize = 64;
/// Bounds an attribute value (`attributes.rs`, `class_list.rs`,
/// `mutation.rs`).
pub(super) const MAX_ATTRIBUTE_VALUE_LENGTH: usize = 4096;
/// Bounds a text/comment node's data (`mutation.rs`, `document.rs`).
pub(super) const MAX_TEXT_NODE_LENGTH: usize = 16 * 1024;
/// Bounds `innerHTML`/`outerHTML`/`insertAdjacentHTML`/`DOMParser` input —
/// real HTML parsing has no natural selector-style traversal cap to reuse,
/// so the input string length is the bound (same "cap the untrusted
/// string, not the derived work" approach `MAX_ATTRIBUTE_VALUE_LENGTH`/
/// `MAX_TEXT_NODE_LENGTH` already take).
pub(super) const MAX_HTML_LENGTH: usize = 64 * 1024;

pub(super) type Getter =
  unsafe extern "C" fn(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue;
pub(super) type Setter = unsafe extern "C" fn(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  val: sys::JSValue,
) -> sys::JSValue;

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
