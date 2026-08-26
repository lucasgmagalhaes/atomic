//! `HTMLElement.dataset`: a read-only-write reflection of every `data-*`
//! attribute as a camelCase own property on an identity-cached plain object
//! (`DATASET_OBJECTS`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_id};
use super::util::Getter;

thread_local! {
    static DATASET_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
    /// Last synced camelCase key set per `(ctx, NodeId)` dataset object, so
    /// `sync_dataset` can clear a key whose backing `data-*` attribute was
    /// removed since the last sync (same "diff against what was there
    /// before" need `class_list::CLASS_LIST_LENGTHS` has, just keyed by name
    /// instead of a plain trailing count since dataset keys aren't densely
    /// indexed).
    static DATASET_KEYS: RefCell<HashMap<usize, HashMap<dom::NodeId, Vec<String>>>> = RefCell::new(HashMap::new());
}

/// Evicts this node's cached `dataset` object and its synced-keys entry —
/// called from `node_registry::evict_node_object`.
pub(super) unsafe fn evict(ctx: *mut sys::JSContext, id: dom::NodeId) {
  let cached = DATASET_OBJECTS.with(|reg| {
    reg
      .borrow_mut()
      .get_mut(&(ctx as usize))
      .and_then(|nodes| nodes.remove(&id))
  });
  if let Some(object) = cached {
    sys::JS_FreeValue(ctx, object);
  }
  DATASET_KEYS.with(|reg| {
    if let Some(nodes) = reg.borrow_mut().get_mut(&(ctx as usize)) {
      nodes.remove(&id);
    }
  });
}

/// Frees every cached `dataset` object for `ctx` — called from
/// `node_registry::cleanup`.
pub(super) unsafe fn cleanup(ctx: *mut sys::JSContext) {
  if let Some(objects) = DATASET_OBJECTS.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
    for (_, obj) in objects {
      sys::JS_FreeValue(ctx, obj);
    }
  }
  DATASET_KEYS.with(|reg| {
    reg.borrow_mut().remove(&(ctx as usize));
  });
}

/// `data-foo-bar` -> `fooBar`, the reverse of `HTMLElement.dataset`'s own
/// attribute-name mapping (`camelCase -> kebab-case` for writes, which this
/// crate doesn't need since dataset writes aren't wired back to attributes —
/// see `sync_dataset`'s doc comment).
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

/// Refreshes a `dataset` object's own properties from every live `data-*`
/// attribute on `id`, clearing a camelCase key whose backing attribute was
/// removed since the last sync. Read-only reflection: `el.dataset.foo =
/// "x"` sets a plain JS property on the cached object but doesn't write a
/// `data-foo` attribute back, since that would need per-key native
/// accessors (or a `Proxy`, which this crate's `quickjs-sys` bindings don't
/// expose) rather than the fixed getter/setter pairs `id`/`className`/etc.
/// use — same documented scope-down as `localStorage`'s missing
/// `localStorage.foo` bracket access.
unsafe fn sync_dataset(
  ctx: *mut sys::JSContext,
  dom: *mut dom::Dom,
  id: dom::NodeId,
  object: sys::JSValue,
) {
  let Some(dom::NodeData::Element { attributes, .. }) = (*dom).get(id).map(|node| &node.data)
  else {
    return;
  };
  let mut current = Vec::new();
  for (name, value) in attributes.iter() {
    let Some(rest) = name.strip_prefix("data-") else {
      continue;
    };
    if rest.is_empty() {
      continue;
    }
    let camel = kebab_to_camel(rest);
    let key = CString::new(camel.clone()).unwrap();
    sys::JS_SetPropertyStr(
      ctx,
      object,
      key.as_ptr(),
      super::util::new_js_string(ctx, value),
    );
    current.push(camel);
  }
  let stale = DATASET_KEYS.with(|reg| {
    reg
      .borrow()
      .get(&(ctx as usize))
      .and_then(|nodes| nodes.get(&id))
      .cloned()
  });
  if let Some(previous) = stale {
    for key in previous.iter().filter(|key| !current.contains(key)) {
      let key = CString::new(key.as_str()).unwrap();
      sys::JS_SetPropertyStr(ctx, object, key.as_ptr(), sys::js_undefined());
    }
  }
  DATASET_KEYS.with(|reg| {
    reg
      .borrow_mut()
      .entry(ctx as usize)
      .or_default()
      .insert(id, current)
  });
}

unsafe extern "C" fn node_dataset_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = node_id(ctx, this_val) else {
    return sys::js_undefined();
  };
  if let Some(value) = DATASET_OBJECTS.with(|reg| {
    reg
      .borrow()
      .get(&(ctx as usize))
      .and_then(|objects| objects.get(&id).copied())
  }) {
    let dom = dom_opaque(ctx);
    if !dom.is_null() {
      sync_dataset(ctx, dom, id, value);
    }
    return sys::JS_DupValue(ctx, value);
  }
  let object = sys::JS_NewObject(ctx);
  let dom = dom_opaque(ctx);
  if !dom.is_null() {
    sync_dataset(ctx, dom, id, object);
  }
  DATASET_OBJECTS.with(|reg| {
    reg
      .borrow_mut()
      .entry(ctx as usize)
      .or_default()
      .insert(id, sys::JS_DupValue(ctx, object))
  });
  object
}

pub(super) unsafe fn define_dataset(ctx: *mut sys::JSContext, proto: sys::JSValue) {
  let name = CString::new("dataset").unwrap();
  let getter = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<Getter, sys::JSCFunction>(node_dataset_get),
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
