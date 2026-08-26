//! `getComputedStyle(element)` — a real global function returning a
//! snapshot `CSSStyleDeclaration`-shaped object backed by
//! `HostState.computed_styles`, which a host (`profile-worker`'s
//! `Page::render`, after it runs `layout-engine`'s real cascade resolver
//! against the current DOM) pushes in wholesale via
//! `Context::set_computed_styles` — same "host computes it, context just
//! stores/exposes the result" shape `layout_measurement`'s `Rect`s already
//! use.
//!
//! Real deviation from spec: the returned object is a point-in-time
//! snapshot (plain data properties set once at call time), not a live view
//! that keeps tracking the element — calling `getComputedStyle` again after
//! a mutation (and a host re-render) gives an up-to-date answer, but the
//! *same* returned object never changes after the fact. This sidesteps
//! needing per-instance owner tracking/resync (the pattern
//! `dom_bindings`'s `classList`/`dataset` use) for a value that's already
//! itself a one-shot snapshot of whatever `layout-engine` last computed —
//! genuinely simpler and no less correct than a "live" object nobody could
//! tell apart from a snapshot anyway without re-running layout.
//!
//! An element with no entry in `computed_styles` (never laid out, `display:
//! none`, or a host that never called `set_computed_styles`) gets an empty
//! declaration — every property reads as `""`, same degrade-gracefully
//! convention `layout_measurement`'s all-zero `Rect` already uses.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

fn kebab_to_camel(name: &str) -> String {
  let mut result = String::with_capacity(name.len());
  let mut capitalize = false;
  for ch in name.chars() {
    if ch == '-' {
      capitalize = true;
      continue;
    }
    if capitalize {
      result.extend(ch.to_uppercase());
      capitalize = false;
    } else {
      result.push(ch);
    }
  }
  result
}

unsafe fn empty_string(ctx: *mut sys::JSContext) -> sys::JSValue {
  let empty = CString::new("").unwrap();
  sys::JS_NewStringLen(ctx, empty.as_ptr(), 0)
}

unsafe extern "C" fn get_property_value(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return empty_string(ctx);
  }
  let mut len: usize = 0;
  let c_str = sys::JS_ToCStringLen2(ctx, &mut len, *argv, false);
  if c_str.is_null() {
    return empty_string(ctx);
  }
  let name = std::ffi::CStr::from_ptr(c_str)
    .to_string_lossy()
    .into_owned();
  sys::JS_FreeCString(ctx, c_str);
  let camel = CString::new(kebab_to_camel(&name)).unwrap_or_default();
  let value = sys::JS_GetPropertyStr(ctx, this_val, camel.as_ptr());
  if sys::js_is_exception(&value) || value.tag == sys::JS_TAG_UNDEFINED {
    sys::JS_FreeValue(ctx, value);
    return empty_string(ctx);
  }
  value
}

unsafe extern "C" fn get_computed_style(
  ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let object = sys::JS_NewObject(ctx);
  if argc < 1 {
    return object;
  }
  let Some(id) = crate::dom_bindings::node_id(ctx, *argv) else {
    return object;
  };
  let state = crate::host_state::get(ctx);
  if state.is_null() {
    return object;
  }
  let mut css_text = String::new();
  if let Some(properties) = (*state).computed_styles.get(&id) {
    let mut names: Vec<_> = properties.keys().collect();
    names.sort_unstable();
    for name in names {
      let value = &properties[name];
      let camel = CString::new(kebab_to_camel(name)).unwrap_or_default();
      let js_value = sys::JS_NewStringLen(
        ctx,
        value.as_ptr() as *const std::os::raw::c_char,
        value.len(),
      );
      sys::JS_SetPropertyStr(ctx, object, camel.as_ptr(), js_value);
      css_text.push_str(name);
      css_text.push_str(": ");
      css_text.push_str(value);
      css_text.push_str("; ");
    }
  }
  let css_text_key = CString::new("cssText").unwrap();
  let css_text_value = sys::JS_NewStringLen(
    ctx,
    css_text.as_ptr() as *const std::os::raw::c_char,
    css_text.len(),
  );
  sys::JS_SetPropertyStr(ctx, object, css_text_key.as_ptr(), css_text_value);
  let method_name = CString::new("getPropertyValue").unwrap();
  sys::JS_SetPropertyStr(
    ctx,
    object,
    method_name.as_ptr(),
    sys::JS_NewCFunction2(
      ctx,
      get_property_value,
      method_name.as_ptr(),
      1,
      sys::JS_CFUNC_GENERIC,
      0,
    ),
  );
  object
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
  let global = sys::JS_GetGlobalObject(ctx);
  let name = CString::new("getComputedStyle").unwrap();
  let function = sys::JS_NewCFunction2(
    ctx,
    get_computed_style,
    name.as_ptr(),
    1,
    sys::JS_CFUNC_GENERIC,
    0,
  );
  sys::JS_SetPropertyStr(ctx, global, name.as_ptr(), function);
  sys::JS_FreeValue(ctx, global);
}
