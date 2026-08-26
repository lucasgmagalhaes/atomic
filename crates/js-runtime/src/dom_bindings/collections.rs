//! Live and snapshot node collections: `NodeList`/`HTMLCollection`-shaped
//! array wrappers (`node_list`/`html_collection`) and the selector/tag/class
//! query entry points (`querySelector(All)`/`getElementsBy*`/`matches`/
//! `closest`) exposed on `Node.prototype`.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_class_id_for, node_id, node_object};
use super::selectors::{
  matching_by_class, matching_by_tag, matching_nodes, node_matches_selector, MAX_SELECTOR_VISITS,
};
use super::util::{read_js_string, throw_type_error};

/// Wraps each id in `nodes` as a real `Node` object (via the identity
/// cache) into a fresh JS array, in the given order — the shared tail end
/// of `query_selector_all`/`elements_by_tag_name`/`elements_by_class_name`.
unsafe fn node_array(ctx: *mut sys::JSContext, nodes: Vec<dom::NodeId>) -> sys::JSValue {
  let array = sys::JS_NewArray(ctx);
  let dom = dom_opaque(ctx);
  for (index, node_id) in nodes.into_iter().enumerate() {
    let class_id = node_class_id_for(ctx, dom, node_id);
    sys::JS_SetPropertyUint32(
      ctx,
      array,
      index as u32,
      node_object(ctx, class_id, node_id),
    );
  }
  array
}

/// Wraps `node_array` into an `HTMLCollection`-like object by adding
/// `item(index)` and `namedItem(name)` methods on the array.
/// `namedItem` matches by `id` or `name` attribute.
pub(super) unsafe fn html_collection(
  ctx: *mut sys::JSContext,
  nodes: Vec<dom::NodeId>,
) -> sys::JSValue {
  let array = node_array(ctx, nodes);
  let item_name = CString::new("item").unwrap();
  let item_fn = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<sys::JSCFunction, sys::JSCFunction>(collection_item),
    item_name.as_ptr(),
    1,
    sys::JS_CFUNC_GENERIC,
    0,
  );
  sys::JS_SetPropertyStr(ctx, array, item_name.as_ptr(), item_fn);
  let named_name = CString::new("namedItem").unwrap();
  let named_fn = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<sys::JSCFunction, sys::JSCFunction>(collection_named_item),
    named_name.as_ptr(),
    1,
    sys::JS_CFUNC_GENERIC,
    0,
  );
  sys::JS_SetPropertyStr(ctx, array, named_name.as_ptr(), named_fn);
  array
}

/// `HTMLCollection.prototype.item(index)` — returns the element at the
/// given index, or `null` if out of range.
unsafe extern "C" fn collection_item(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return sys::js_null();
  }
  let val = *argv;
  if val.tag != sys::JS_TAG_INT {
    return sys::js_null();
  }
  let idx = val.u.int32;
  if idx < 0 {
    return sys::js_null();
  }
  let elem = sys::JS_GetPropertyUint32(ctx, this_val, idx as u32);
  if elem.tag == sys::JS_TAG_UNDEFINED {
    sys::js_null()
  } else {
    elem
  }
}

/// `HTMLCollection.prototype.namedItem(name)` — returns the first element
/// whose `id` or `name` attribute matches `name`, or `null` if none.
unsafe extern "C" fn collection_named_item(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return sys::js_null();
  }
  let Some(name) = read_js_string(ctx, *argv) else {
    return sys::js_null();
  };
  if name.is_empty() {
    return sys::js_null();
  }
  let length_prop = CString::new("length").unwrap();
  let length = sys::JS_GetPropertyStr(ctx, this_val, length_prop.as_ptr());
  let len = if length.tag == sys::JS_TAG_INT {
    length.u.int32
  } else {
    0
  };
  sys::JS_FreeValue(ctx, length);
  for i in 0..len {
    let elem = sys::JS_GetPropertyUint32(ctx, this_val, i as u32);
    if elem.tag == sys::JS_TAG_UNDEFINED || elem.tag == sys::JS_TAG_NULL {
      continue;
    }
    if element_matches_name(ctx, elem, &name) {
      return elem;
    }
    sys::JS_FreeValue(ctx, elem);
  }
  sys::js_null()
}

/// Checks if an element's `id` or `name` attribute matches the given name.
unsafe fn element_matches_name(ctx: *mut sys::JSContext, elem: sys::JSValue, name: &str) -> bool {
  let id_atom = CString::new("id").unwrap();
  let id_val = sys::JS_GetPropertyStr(ctx, elem, id_atom.as_ptr());
  let matched = if let Some(id_str) = read_js_string(ctx, id_val) {
    id_str == name
  } else {
    false
  };
  sys::JS_FreeValue(ctx, id_val);
  if matched {
    return true;
  }
  let name_atom = CString::new("name").unwrap();
  let name_val = sys::JS_GetPropertyStr(ctx, elem, name_atom.as_ptr());
  let matched = if let Some(n) = read_js_string(ctx, name_val) {
    n == name
  } else {
    false
  };
  sys::JS_FreeValue(ctx, name_val);
  matched
}

/// Wraps `node_array` into a `NodeList`-like object by adding
/// `item(index)` method on the array.
pub(super) unsafe fn node_list(ctx: *mut sys::JSContext, nodes: Vec<dom::NodeId>) -> sys::JSValue {
  let array = node_array(ctx, nodes);
  let item_name = CString::new("item").unwrap();
  let item_fn = sys::JS_NewCFunction2(
    ctx,
    std::mem::transmute::<sys::JSCFunction, sys::JSCFunction>(collection_item),
    item_name.as_ptr(),
    1,
    sys::JS_CFUNC_GENERIC,
    0,
  );
  sys::JS_SetPropertyStr(ctx, array, item_name.as_ptr(), item_fn);
  array
}

pub(super) unsafe fn query_selector_all(
  ctx: *mut sys::JSContext,
  start: dom::NodeId,
  selector: &str,
  include_start: bool,
) -> sys::JSValue {
  let dom_ptr = dom_opaque(ctx);
  if dom_ptr.is_null() {
    return sys::JS_NewArray(ctx);
  }
  let nodes = match matching_nodes(&*dom_ptr, start, selector, include_start) {
    Ok(nodes) => nodes,
    Err(message) => return throw_type_error(ctx, message),
  };
  node_list(ctx, nodes)
}

pub(super) unsafe fn elements_by_tag_name(
  ctx: *mut sys::JSContext,
  start: dom::NodeId,
  tag: &str,
  include_start: bool,
) -> sys::JSValue {
  let dom_ptr = dom_opaque(ctx);
  if dom_ptr.is_null() {
    return sys::JS_NewArray(ctx);
  }
  match matching_by_tag(&*dom_ptr, start, tag, include_start) {
    Ok(nodes) => html_collection(ctx, nodes),
    Err(message) => throw_type_error(ctx, message),
  }
}

pub(super) unsafe fn elements_by_class_name(
  ctx: *mut sys::JSContext,
  start: dom::NodeId,
  class_name: &str,
  include_start: bool,
) -> sys::JSValue {
  let dom_ptr = dom_opaque(ctx);
  if dom_ptr.is_null() {
    return sys::JS_NewArray(ctx);
  }
  match matching_by_class(&*dom_ptr, start, class_name, include_start) {
    Ok(nodes) => html_collection(ctx, nodes),
    Err(message) => throw_type_error(ctx, message),
  }
}

unsafe extern "C" fn node_query_selector(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return throw_type_error(ctx, "selector is required");
  }
  let Some(selector) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "selector must be a string");
  };
  let Some(start) = node_id(ctx, this_val) else {
    return sys::js_null();
  };
  let all = query_selector_all(ctx, start, &selector, false);
  if sys::js_is_exception(&all) {
    return all;
  }
  let first = sys::JS_GetPropertyUint32(ctx, all, 0);
  sys::JS_FreeValue(ctx, all);
  if first.tag == sys::JS_TAG_UNDEFINED {
    sys::JS_FreeValue(ctx, first);
    sys::js_null()
  } else {
    first
  }
}

unsafe extern "C" fn node_query_selector_all(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return throw_type_error(ctx, "selector is required");
  }
  let Some(selector) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "selector must be a string");
  };
  let Some(start) = node_id(ctx, this_val) else {
    return sys::JS_NewArray(ctx);
  };
  query_selector_all(ctx, start, &selector, false)
}

unsafe extern "C" fn node_get_elements_by_tag_name(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return throw_type_error(ctx, "tag name is required");
  }
  let Some(tag) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "tag name must be a string");
  };
  let Some(start) = node_id(ctx, this_val) else {
    return sys::JS_NewArray(ctx);
  };
  elements_by_tag_name(ctx, start, &tag, false)
}

unsafe extern "C" fn node_get_elements_by_class_name(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return throw_type_error(ctx, "class name is required");
  }
  let Some(class_name) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "class name must be a string");
  };
  let Some(start) = node_id(ctx, this_val) else {
    return sys::JS_NewArray(ctx);
  };
  elements_by_class_name(ctx, start, &class_name, false)
}

unsafe extern "C" fn node_matches(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return throw_type_error(ctx, "selector is required");
  }
  let Some(selector) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "selector must be a string");
  };
  let Some(id) = node_id(ctx, this_val) else {
    return sys::js_bool(false);
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() {
    return sys::js_bool(false);
  }
  match node_matches_selector(&*dom, id, &selector) {
    Ok(matches) => sys::js_bool(matches),
    Err(message) => throw_type_error(ctx, message),
  }
}

unsafe extern "C" fn node_closest(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  if argc < 1 {
    return throw_type_error(ctx, "selector is required");
  }
  let Some(selector) = read_js_string(ctx, *argv) else {
    return throw_type_error(ctx, "selector must be a string");
  };
  let Some(mut current) = node_id(ctx, this_val) else {
    return sys::js_null();
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() {
    return sys::js_null();
  }
  for _ in 0..MAX_SELECTOR_VISITS {
    match node_matches_selector(&*dom, current, &selector) {
      Ok(true) => {
        let class_id = node_class_id_for(ctx, dom, current);
        return node_object(ctx, class_id, current);
      }
      Ok(false) => {}
      Err(message) => return throw_type_error(ctx, message),
    }
    let Some(parent) = (*dom).get(current).and_then(|node| node.parent) else {
      return sys::js_null();
    };
    current = parent;
  }
  throw_type_error(ctx, "selector traversal limit exceeded")
}

pub(super) unsafe fn define_selector_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
  for (name, function) in [
    ("querySelector", node_query_selector as sys::JSCFunction),
    (
      "querySelectorAll",
      node_query_selector_all as sys::JSCFunction,
    ),
    ("matches", node_matches as sys::JSCFunction),
    ("closest", node_closest as sys::JSCFunction),
    (
      "getElementsByTagName",
      node_get_elements_by_tag_name as sys::JSCFunction,
    ),
    (
      "getElementsByClassName",
      node_get_elements_by_class_name as sys::JSCFunction,
    ),
  ] {
    let name = CString::new(name).unwrap();
    let value = sys::JS_NewCFunction2(ctx, function, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, proto, name.as_ptr(), value);
  }
}
