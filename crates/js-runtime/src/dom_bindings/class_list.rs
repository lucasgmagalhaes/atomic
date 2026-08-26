//! `Element.classList`: a live, identity-cached `DOMTokenList`-shaped object
//! (`CLASS_LIST_OBJECTS`) backed by the node's own `class` attribute, plus
//! the shared `Array.prototype`-borrowing trick (`array_prototype`) that
//! `attributes.rs`'s `NamedNodeMap`-shaped `attributes` collection also
//! reuses.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_id};
use super::util::{new_js_string, read_js_string, throw_type_error, Getter, Setter};

thread_local! {
    static CLASS_LIST_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
    /// Last synced token count per `(ctx, NodeId)` classList object, so
    /// `sync_class_list` knows how many trailing indexed properties (from a
    /// shrunk class attribute) need clearing to `undefined` rather than left
    /// stale from a longer previous token list.
    static CLASS_LIST_LENGTHS: RefCell<HashMap<usize, HashMap<dom::NodeId, usize>>> = RefCell::new(HashMap::new());
}

/// Evicts this node's cached `classList` object and its synced-length entry
/// — called from `node_registry::evict_node_object`.
pub(super) unsafe fn evict(ctx: *mut sys::JSContext, id: dom::NodeId) {
  let cached = CLASS_LIST_OBJECTS.with(|reg| {
    reg
      .borrow_mut()
      .get_mut(&(ctx as usize))
      .and_then(|nodes| nodes.remove(&id))
  });
  if let Some(object) = cached {
    sys::JS_FreeValue(ctx, object);
  }
  CLASS_LIST_LENGTHS.with(|reg| {
    if let Some(nodes) = reg.borrow_mut().get_mut(&(ctx as usize)) {
      nodes.remove(&id);
    }
  });
}

/// Frees every cached `classList` object for `ctx` — called from
/// `node_registry::cleanup`.
pub(super) unsafe fn cleanup(ctx: *mut sys::JSContext) {
  if let Some(objects) = CLASS_LIST_OBJECTS.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
    for (_, obj) in objects {
      sys::JS_FreeValue(ctx, obj);
    }
  }
  CLASS_LIST_LENGTHS.with(|reg| {
    reg.borrow_mut().remove(&(ctx as usize));
  });
}

unsafe fn class_list_owner(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<dom::NodeId> {
  let name = CString::new("__nimbleClassListOwner").unwrap();
  let owner = sys::JS_GetPropertyStr(ctx, value, name.as_ptr());
  let id = node_id(ctx, owner);
  sys::JS_FreeValue(ctx, owner);
  id
}

/// Also used by `selectors.rs`'s `matching_by_class`, which needs the same
/// whitespace-tokenizing rule for a class-name argument that isn't backed
/// by any live node.
pub(super) fn class_tokens(value: &str) -> Vec<String> {
  value.split_ascii_whitespace().map(str::to_owned).collect()
}

fn valid_class_token(token: &str) -> bool {
  !token.is_empty()
    && token.len() <= super::util::MAX_ATTRIBUTE_NAME_LENGTH
    && !token.bytes().any(|byte| byte.is_ascii_whitespace())
}

/// Refreshes a classList object's indexed properties (`0`, `1`, ...) and
/// `length` from the node's live `class` attribute, clearing any trailing
/// index left over from a longer previous token list. Called on every
/// mutation and every `element.classList` getter hit, so a cached wrapper
/// (identity-stable per `NodeId`, see `CLASS_LIST_OBJECTS`) never serves
/// stale indices after a direct `setAttribute("class", ...)` bypassed it.
unsafe fn sync_class_list(
  ctx: *mut sys::JSContext,
  dom: *mut dom::Dom,
  id: dom::NodeId,
  object: sys::JSValue,
) {
  let tokens = class_tokens((*dom).attribute(id, "class").unwrap_or_default());
  let old_len = CLASS_LIST_LENGTHS
    .with(|reg| {
      reg
        .borrow()
        .get(&(ctx as usize))
        .and_then(|nodes| nodes.get(&id).copied())
    })
    .unwrap_or(0);
  for (index, token) in tokens.iter().enumerate() {
    sys::JS_SetPropertyUint32(ctx, object, index as u32, new_js_string(ctx, token));
  }
  for index in tokens.len()..old_len {
    sys::JS_SetPropertyUint32(ctx, object, index as u32, sys::js_undefined());
  }
  let length_name = CString::new("length").unwrap();
  sys::JS_SetPropertyStr(
    ctx,
    object,
    length_name.as_ptr(),
    sys::js_float64(tokens.len() as f64),
  );
  CLASS_LIST_LENGTHS.with(|reg| {
    reg
      .borrow_mut()
      .entry(ctx as usize)
      .or_default()
      .insert(id, tokens.len())
  });
}

/// Fetches `Array.prototype` so a classList object can inherit it via
/// `JS_SetPrototype` — the cheapest real way to get a genuine, spec-shaped
/// `Symbol.iterator`/`forEach`/`entries`/... for free on an array-like
/// object, since this crate's `quickjs-sys` bindings don't expose a way to
/// define a well-known-symbol property directly.
///
/// Also used by `attributes.rs`'s `node_attributes_get` for the same
/// `NamedNodeMap`-shaped array-like object.
pub(super) unsafe fn array_prototype(ctx: *mut sys::JSContext) -> sys::JSValue {
  let global = sys::JS_GetGlobalObject(ctx);
  let array_name = CString::new("Array").unwrap();
  let array_ctor = sys::JS_GetPropertyStr(ctx, global, array_name.as_ptr());
  sys::JS_FreeValue(ctx, global);
  let proto_name = CString::new("prototype").unwrap();
  let proto = sys::JS_GetPropertyStr(ctx, array_ctor, proto_name.as_ptr());
  sys::JS_FreeValue(ctx, array_ctor);
  proto
}

unsafe fn class_list_mutate(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
  add: bool,
) -> sys::JSValue {
  let Some(id) = class_list_owner(ctx, this_val) else {
    return throw_type_error(ctx, "invalid classList receiver");
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() || (*dom).get(id).is_none() {
    return throw_type_error(ctx, "node is no longer attached to this document");
  }
  let mut tokens = class_tokens((*dom).attribute(id, "class").unwrap_or_default());
  for index in 0..argc {
    let Some(token) = read_js_string(ctx, *argv.add(index as usize)) else {
      return throw_type_error(ctx, "class token must be a string");
    };
    if !valid_class_token(&token) {
      return throw_type_error(ctx, "invalid class token");
    }
    if add {
      if !tokens.contains(&token) {
        tokens.push(token);
      }
    } else {
      tokens.retain(|current| current != &token);
    }
  }
  let value = tokens.join(" ");
  if value.len() > super::util::MAX_ATTRIBUTE_VALUE_LENGTH {
    return throw_type_error(ctx, "class attribute exceeds the maximum length");
  }
  (*dom).set_attribute(id, "class", &value);
  sync_class_list(ctx, dom, id, this_val);
  sys::js_undefined()
}

unsafe extern "C" fn class_list_add(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  class_list_mutate(ctx, this_val, argc, argv, true)
}
unsafe extern "C" fn class_list_remove(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  class_list_mutate(ctx, this_val, argc, argv, false)
}
unsafe extern "C" fn class_list_contains(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return throw_type_error(ctx, "class token is required");
  }
  let Some(token) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "class token must be a string");
  };
  if !valid_class_token(&token) {
    return throw_type_error(ctx, "invalid class token");
  }
  let Some(id) = class_list_owner(ctx, this_val) else {
    return sys::js_bool(false);
  };
  let dom = dom_opaque(ctx);
  sys::js_bool(
    !dom.is_null()
      && class_tokens((*dom).attribute(id, "class").unwrap_or_default()).contains(&token),
  )
}

unsafe extern "C" fn class_list_toggle(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return throw_type_error(ctx, "class token is required");
  }
  let Some(token) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "class token must be a string");
  };
  if !valid_class_token(&token) {
    return throw_type_error(ctx, "invalid class token");
  }
  let Some(id) = class_list_owner(ctx, this_val) else {
    return throw_type_error(ctx, "invalid classList receiver");
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() || (*dom).get(id).is_none() {
    return throw_type_error(ctx, "node is no longer attached to this document");
  }
  let mut tokens = class_tokens((*dom).attribute(id, "class").unwrap_or_default());
  let present = tokens.contains(&token);
  let force = if argc >= 2 {
    Some(sys::JS_ToBool(ctx, *argv.add(1)) != 0)
  } else {
    None
  };
  let should_be_present = force.unwrap_or(!present);
  if should_be_present && !present {
    tokens.push(token);
  } else if !should_be_present && present {
    tokens.retain(|current| current != &token);
  }
  let value = tokens.join(" ");
  if value.len() > super::util::MAX_ATTRIBUTE_VALUE_LENGTH {
    return throw_type_error(ctx, "class attribute exceeds the maximum length");
  }
  (*dom).set_attribute(id, "class", &value);
  sync_class_list(ctx, dom, id, this_val);
  sys::js_bool(should_be_present)
}

unsafe extern "C" fn class_list_replace(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 2 {
    return throw_type_error(ctx, "old and new class tokens are required");
  }
  let Some(old_token) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "class token must be a string");
  };
  let Some(new_token) = read_js_string(ctx, *argv.add(1)) else {
    return throw_type_error(ctx, "class token must be a string");
  };
  if !valid_class_token(&old_token) || !valid_class_token(&new_token) {
    return throw_type_error(ctx, "invalid class token");
  }
  let Some(id) = class_list_owner(ctx, this_val) else {
    return throw_type_error(ctx, "invalid classList receiver");
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() || (*dom).get(id).is_none() {
    return throw_type_error(ctx, "node is no longer attached to this document");
  }
  let mut tokens = class_tokens((*dom).attribute(id, "class").unwrap_or_default());
  let Some(position) = tokens.iter().position(|current| current == &old_token) else {
    return sys::js_bool(false);
  };
  tokens[position] = new_token;
  let value = tokens.join(" ");
  if value.len() > super::util::MAX_ATTRIBUTE_VALUE_LENGTH {
    return throw_type_error(ctx, "class attribute exceeds the maximum length");
  }
  (*dom).set_attribute(id, "class", &value);
  sync_class_list(ctx, dom, id, this_val);
  sys::js_bool(true)
}

unsafe extern "C" fn class_list_value_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = class_list_owner(ctx, this_val) else {
    return new_js_string(ctx, "");
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() {
    return new_js_string(ctx, "");
  }
  new_js_string(ctx, (*dom).attribute(id, "class").unwrap_or_default())
}

unsafe extern "C" fn class_list_value_set(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argv: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = class_list_owner(ctx, this_val) else {
    return throw_type_error(ctx, "invalid classList receiver");
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() || (*dom).get(id).is_none() {
    return throw_type_error(ctx, "node is no longer attached to this document");
  }
  let Some(value) = read_js_string(ctx, argv) else {
    return throw_type_error(ctx, "classList.value must be a string");
  };
  if value.len() > super::util::MAX_ATTRIBUTE_VALUE_LENGTH {
    return throw_type_error(ctx, "class attribute exceeds the maximum length");
  }
  (*dom).set_attribute(id, "class", &value);
  sync_class_list(ctx, dom, id, this_val);
  sys::js_undefined()
}

unsafe extern "C" fn node_class_list_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = node_id(ctx, this_val) else {
    return sys::js_undefined();
  };
  if let Some(value) = CLASS_LIST_OBJECTS.with(|reg| {
    reg
      .borrow()
      .get(&(ctx as usize))
      .and_then(|objects| objects.get(&id).copied())
  }) {
    let dom = dom_opaque(ctx);
    if !dom.is_null() {
      sync_class_list(ctx, dom, id, value);
    }
    return sys::JS_DupValue(ctx, value);
  }
  let object = sys::JS_NewObject(ctx);
  let proto = array_prototype(ctx);
  sys::JS_SetPrototype(ctx, object, proto);
  sys::JS_FreeValue(ctx, proto);
  let owner = CString::new("__nimbleClassListOwner").unwrap();
  sys::JS_SetPropertyStr(ctx, object, owner.as_ptr(), sys::JS_DupValue(ctx, this_val));
  for (name, function) in [
    ("add", class_list_add as sys::JSCFunction),
    ("remove", class_list_remove as sys::JSCFunction),
    ("contains", class_list_contains as sys::JSCFunction),
    ("toggle", class_list_toggle as sys::JSCFunction),
    ("replace", class_list_replace as sys::JSCFunction),
  ] {
    let name = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(
      ctx,
      object,
      name.as_ptr(),
      sys::JS_NewCFunction2(ctx, function, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0),
    );
  }
  let value_name = CString::new("value").unwrap();
  let value_getter = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<Getter, sys::JSCFunction>(class_list_value_get),
    value_name.as_ptr(),
    0,
    sys::JS_CFUNC_GETTER,
    0,
  );
  let value_setter = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<Setter, sys::JSCFunction>(class_list_value_set),
    value_name.as_ptr(),
    1,
    sys::JS_CFUNC_SETTER,
    0,
  );
  let value_atom = sys::JS_NewAtom(ctx, value_name.as_ptr());
  sys::JS_DefinePropertyGetSet(
    ctx,
    object,
    value_atom,
    value_getter,
    value_setter,
    sys::JS_PROP_HAS_GET | sys::JS_PROP_HAS_SET | sys::JS_PROP_CONFIGURABLE,
  );
  sys::JS_FreeAtom(ctx, value_atom);
  let dom = dom_opaque(ctx);
  if !dom.is_null() {
    sync_class_list(ctx, dom, id, object);
  }
  CLASS_LIST_OBJECTS.with(|reg| {
    reg
      .borrow_mut()
      .entry(ctx as usize)
      .or_default()
      .insert(id, sys::JS_DupValue(ctx, object))
  });
  object
}

pub(super) unsafe fn define_class_list(ctx: *mut sys::JSContext, proto: sys::JSValue) {
  let name = CString::new("classList").unwrap();
  let getter = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<Getter, sys::JSCFunction>(node_class_list_get),
    name.as_ptr(),
    0,
    sys::JS_CFUNC_GETTER,
    0,
  );
  let atom = sys::JS_NewAtom(ctx, name.as_ptr());
  sys::JS_DefinePropertyGetSet(
    ctx,
    proto,
    atom,
    getter,
    sys::js_undefined(),
    sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
  );
  sys::JS_FreeAtom(ctx, atom);
}
