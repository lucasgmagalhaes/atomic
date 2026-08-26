//! `CSSStyleSheet` — real, spec-shaped rule storage and mutation
//! (`insertRule`/`deleteRule`/`cssRules`), plus `document.adoptedStyleSheets`
//! as a plain writable array a page assigns sheets into. Closes the
//! "stylesheet mutation/adopted stylesheets" half of the CSSOM gap
//! `JS_ENGINE_CAPABILITY_MATRIX.md` item 6 still listed as not done
//! (`element.style`/`getComputedStyle` — the other two CSSOM pieces —
//! already existed before this).
//!
//! Real wiring, not just a JS-facing shell: `Context::adopted_stylesheet_text`
//! lets a host (`profile-worker`'s `Page::layout`) read every adopted
//! sheet's accumulated rule text back out and merge it into the real
//! `layout-engine` cascade before the next layout pass — mutating a sheet
//! via `insertRule`/`deleteRule` and re-rendering actually changes what
//! gets painted, tested end to end in `crates/profile/tests`.
//!
//! Scope cuts: each rule is stored as the plain source text it was
//! inserted with (`Vec<String>`, one entry per rule) rather than a real
//! `CSSRule`/`CSSStyleRule` object graph — `cssRules[i]` is a snapshot
//! `{ cssText }`, not a live object whose own `.style` can be mutated
//! (matching `getAttributeNames`' "attributes sorted, no live `Attr`
//! nodes" convention already documented elsewhere in this crate).
//! `insertRule` validates via `css::parse_stylesheet` producing exactly
//! one rule; a malformed or multi-rule string throws, matching the real
//! API's `SyntaxError`. No `replace`/`replaceSync` (whole-sheet-text
//! replacement) — splitting an arbitrary stylesheet string back into
//! individually addressable rule texts needs source-span tracking `css`'s
//! parser doesn't keep, a documented scope cut rather than storing the
//! whole thing as one opaque blob rule (which would misrepresent
//! `cssRules.length`). `document.adoptedStyleSheets` does no validation
//! that its entries are actually `CSSStyleSheet` instances — a non-sheet
//! entry is silently skipped by `adopted_stylesheet_text`, same
//! degrade-gracefully convention as everything else reading page-provided
//! state.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

const STYLESHEET_CLASS_KIND: &str = "CSSStyleSheet";
const MAX_RULE_LENGTH: usize = 8192;
const MAX_RULES_PER_SHEET: usize = 4096;

unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
  sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
  let mut len: usize = 0;
  let ptr = sys::JS_ToCStringLen2(ctx, &mut len, val, false);
  if ptr.is_null() {
    return None;
  }
  let s = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
  sys::JS_FreeCString(ctx, ptr);
  Some(s)
}

unsafe fn throw_type_error(ctx: *mut sys::JSContext, message: &str) -> sys::JSValue {
  sys::JS_Throw(ctx, new_js_string(ctx, message))
}

fn read_js_number(val: sys::JSValue) -> Option<f64> {
  match val.tag {
    sys::JS_TAG_INT => Some(unsafe { val.u.int32 } as f64),
    sys::JS_TAG_FLOAT64 => Some(unsafe { val.u.float64 }),
    _ => None,
  }
}

unsafe fn sheet_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut Vec<String> {
  let class_id = crate::class_registry::class_id_for(rt, STYLESHEET_CLASS_KIND);
  sys::JS_GetOpaque(this_val, class_id) as *mut Vec<String>
}

unsafe extern "C" fn stylesheet_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
  let ptr = sheet_opaque(rt, val);
  if !ptr.is_null() {
    drop(Box::from_raw(ptr));
  }
}

unsafe extern "C" fn stylesheet_constructor(
  ctx: *mut sys::JSContext,
  new_target: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  let class_id =
    crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), STYLESHEET_CLASS_KIND);
  let obj = sys::JS_NewObjectClass(ctx, class_id);
  if sys::js_is_exception(&obj) {
    return obj;
  }
  if new_target.tag != sys::JS_TAG_UNDEFINED {
    let proto_name = CString::new("prototype").unwrap();
    let proto = sys::JS_GetPropertyStr(ctx, new_target, proto_name.as_ptr());
    if proto.tag != sys::JS_TAG_UNDEFINED {
      sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);
  }
  sys::JS_SetOpaque(
    obj,
    Box::into_raw(Box::<Vec<String>>::default()) as *mut std::os::raw::c_void,
  );
  obj
}

/// Validates `text` parses to exactly one real rule via `css`'s own
/// tokenizer/parser — the same validation a real `insertRule` performs
/// (rejecting empty input, garbage, or more than one rule) before
/// accepting it.
fn is_single_valid_rule(text: &str) -> bool {
  !text.trim().is_empty() && css::parse_stylesheet(text).rules.len() == 1
}

unsafe extern "C" fn insert_rule(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let ptr = sheet_opaque(sys::JS_GetRuntime(ctx), this_val);
  if ptr.is_null() {
    return throw_type_error(ctx, "invalid CSSStyleSheet receiver");
  }
  if argc < 1 {
    return throw_type_error(ctx, "a rule string is required");
  }
  let Some(text) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "rule must be a string");
  };
  if text.len() > MAX_RULE_LENGTH {
    return throw_type_error(ctx, "rule text exceeds the maximum length");
  }
  if !is_single_valid_rule(&text) {
    return throw_type_error(ctx, "rule does not parse as exactly one CSS rule");
  }
  let rules = &mut *ptr;
  if rules.len() >= MAX_RULES_PER_SHEET {
    return throw_type_error(ctx, "stylesheet has reached the maximum rule count");
  }
  let index = if argc >= 2 {
    let n = read_js_number(*argv.add(1)).unwrap_or(0.0) as i64;
    (n.max(0) as usize).min(rules.len())
  } else {
    rules.len()
  };
  rules.insert(index, text);
  sys::js_float64(index as f64)
}

unsafe extern "C" fn delete_rule(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  let ptr = sheet_opaque(sys::JS_GetRuntime(ctx), this_val);
  if ptr.is_null() {
    return throw_type_error(ctx, "invalid CSSStyleSheet receiver");
  }
  let rules = &mut *ptr;
  let index = if argc >= 1 {
    read_js_number(*argv).unwrap_or(-1.0) as i64
  } else {
    -1
  };
  if index < 0 || index as usize >= rules.len() {
    return throw_type_error(ctx, "rule index out of range");
  }
  rules.remove(index as usize);
  sys::js_undefined()
}

unsafe extern "C" fn css_rules_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let array = sys::JS_NewArray(ctx);
  let ptr = sheet_opaque(sys::JS_GetRuntime(ctx), this_val);
  if ptr.is_null() {
    return array;
  }
  for (index, text) in (*ptr).iter().enumerate() {
    let entry = sys::JS_NewObject(ctx);
    let key = CString::new("cssText").unwrap();
    sys::JS_SetPropertyStr(ctx, entry, key.as_ptr(), new_js_string(ctx, text));
    sys::JS_SetPropertyUint32(ctx, array, index as u32, entry);
  }
  array
}

unsafe fn define_getter(
  ctx: *mut sys::JSContext,
  proto: sys::JSValue,
  name: &str,
  getter: unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue) -> sys::JSValue,
) {
  let name_c = CString::new(name).unwrap();
  type Getter =
    unsafe extern "C" fn(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue;
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
    proto,
    atom,
    f,
    sys::js_undefined(),
    sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
  );
  sys::JS_FreeAtom(ctx, atom);
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

/// Every real navigated document's own opaque data pointer identifies a
/// `CSSStyleSheet` instance the same way [`sheet_opaque`] does elsewhere —
/// used by [`crate::Context::adopted_stylesheet_text`] to tell a real
/// sheet apart from any other value a page might have put into
/// `document.adoptedStyleSheets`.
pub(crate) unsafe fn rules_of(
  ctx: *mut sys::JSContext,
  value: sys::JSValue,
) -> Option<Vec<String>> {
  let ptr = sheet_opaque(sys::JS_GetRuntime(ctx), value);
  (!ptr.is_null()).then(|| (*ptr).clone())
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
  let rt = sys::JS_GetRuntime(ctx);
  let class_name = CString::new("CSSStyleSheet").unwrap();
  let def = sys::JSClassDef {
    class_name: class_name.as_ptr(),
    finalizer: Some(stylesheet_finalizer),
    gc_mark: std::ptr::null_mut(),
    call: std::ptr::null_mut(),
    exotic: std::ptr::null_mut(),
  };
  let class_id = crate::class_registry::ensure_class(rt, STYLESHEET_CLASS_KIND, &def);

  let proto = sys::JS_NewObject(ctx);
  define_getter(ctx, proto, "cssRules", css_rules_get);
  define_method(ctx, proto, "insertRule", insert_rule, 2);
  define_method(ctx, proto, "deleteRule", delete_rule, 1);

  let ctor_name = CString::new("CSSStyleSheet").unwrap();
  let ctor_fn = sys::JS_NewCFunction2(
    ctx,
    stylesheet_constructor,
    ctor_name.as_ptr(),
    0,
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

  let document = crate::document::get_or_create(ctx);
  let adopted_name = CString::new("adoptedStyleSheets").unwrap();
  sys::JS_SetPropertyStr(ctx, document, adopted_name.as_ptr(), sys::JS_NewArray(ctx));
  sys::JS_FreeValue(ctx, document);
}
