//! Shared `document` global lookup, used by both `dom_bindings` (adds
//! `getElementById`) and `page_visibility` (adds `visibilityState`/
//! `hidden`) so whichever registers second doesn't clobber the object the
//! first one already installed.
use std::ffi::CString;

use quickjs_sys as sys;

/// Returns the global `document` object, creating and installing an empty
/// one on first use. Caller owns the returned reference and must
/// `JS_FreeValue` it once done (same convention as `JS_GetGlobalObject`).
pub(crate) unsafe fn get_or_create(ctx: *mut sys::JSContext) -> sys::JSValue {
  let global = sys::JS_GetGlobalObject(ctx);
  let name = CString::new("document").unwrap();

  let existing = sys::JS_GetPropertyStr(ctx, global, name.as_ptr());
  let document = if existing.tag == sys::JS_TAG_UNDEFINED {
    sys::JS_FreeValue(ctx, existing);
    let doc = sys::JS_NewObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, name.as_ptr(), sys::JS_DupValue(ctx, doc));
    doc
  } else {
    existing
  };

  sys::JS_FreeValue(ctx, global);
  document
}
