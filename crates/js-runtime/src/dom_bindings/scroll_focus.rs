//! Scroll stubs (this engine has no real viewport) and real `focus()`/
//! `blur()`, both defined on `Element.prototype`/`Node.prototype`
//! respectively.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_opaque};
use super::util::{Getter, Setter};

/// Defines scroll-related properties and methods on `Element.prototype`.
/// This engine has no real viewport, so these are stubs: `scrollTop`/`scrollLeft`
/// always return 0, setters are no-ops, and `scroll()`/`scrollTo()`/`scrollBy()`/
/// `scrollIntoView()` are no-ops. Exists so real-world code that calls these
/// methods doesn't throw.
pub(super) unsafe fn define_scroll_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
  for name in ["scrollTop", "scrollLeft"] {
    let cname = CString::new(name).unwrap();
    let getter = sys::JS_NewCFunction2(
      ctx,
      std::mem::transmute::<Getter, sys::JSCFunction>(scroll_offset_get),
      cname.as_ptr(),
      0,
      sys::JS_CFUNC_GETTER,
      0,
    );
    let setter = sys::JS_NewCFunction2(
      ctx,
      std::mem::transmute::<Setter, sys::JSCFunction>(scroll_offset_set),
      cname.as_ptr(),
      1,
      sys::JS_CFUNC_SETTER,
      0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
      ctx,
      proto,
      atom,
      getter,
      setter,
      sys::JS_PROP_HAS_GET
        | sys::JS_PROP_HAS_SET
        | sys::JS_PROP_CONFIGURABLE
        | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
  }

  for (name, func) in [
    ("scroll", element_scroll_noop as sys::JSCFunction),
    ("scrollTo", element_scroll_noop as sys::JSCFunction),
    ("scrollBy", element_scroll_noop as sys::JSCFunction),
    (
      "scrollIntoView",
      element_scroll_into_view_noop as sys::JSCFunction,
    ),
  ] {
    let cname = CString::new(name).unwrap();
    let value = sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), 0, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, proto, cname.as_ptr(), value);
  }
}

unsafe extern "C" fn scroll_offset_get(
  _ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
) -> sys::JSValue {
  sys::JSValue {
    u: sys::JSValueUnion { int32: 0 },
    tag: sys::JS_TAG_INT,
  }
}

unsafe extern "C" fn scroll_offset_set(
  _ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
  _val: sys::JSValue,
) -> sys::JSValue {
  sys::js_undefined()
}

/// Also used by `forms.rs`'s `define_form_properties` for the no-op
/// `submit()`/`reset()` methods.
pub(super) unsafe extern "C" fn element_scroll_noop(
  _ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  sys::js_undefined()
}

unsafe extern "C" fn element_scroll_into_view_noop(
  _ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  sys::js_undefined()
}

unsafe extern "C" fn node_focus(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
  let dom_ptr = dom_opaque(ctx);
  if !node_ptr.is_null() && !dom_ptr.is_null() {
    (*dom_ptr).focus(*node_ptr);
    crate::events::dispatch(ctx, this_val, "focus");
  }
  sys::js_undefined()
}

unsafe extern "C" fn node_blur(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
  let dom_ptr = dom_opaque(ctx);
  if !node_ptr.is_null() && !dom_ptr.is_null() {
    // `Some(changed)` only when `id` really was the focused node —
    // matches the real DOM not firing `"blur"` for an element that
    // wasn't focused to begin with.
    if let Some(changed) = (*dom_ptr).blur(*node_ptr) {
      crate::events::dispatch(ctx, this_val, "blur");
      if changed {
        crate::events::dispatch(ctx, this_val, "change");
      }
    }
  }
  sys::js_undefined()
}

/// Defines real `focus()`/`blur()` methods on `proto`, backed by
/// `dom::Dom`'s own focus state (`document.activeElement`, see
/// `document::document_active_element_get`).
pub(super) unsafe fn define_focus_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
  let focus_name = CString::new("focus").unwrap();
  let focus_fn = sys::JS_NewCFunction2(
    ctx,
    node_focus,
    focus_name.as_ptr(),
    0,
    sys::JS_CFUNC_GENERIC,
    0,
  );
  sys::JS_SetPropertyStr(ctx, proto, focus_name.as_ptr(), focus_fn);

  let blur_name = CString::new("blur").unwrap();
  let blur_fn = sys::JS_NewCFunction2(
    ctx,
    node_blur,
    blur_name.as_ptr(),
    0,
    sys::JS_CFUNC_GENERIC,
    0,
  );
  sys::JS_SetPropertyStr(ctx, proto, blur_name.as_ptr(), blur_fn);
}
