//! Read-only tree-navigation accessors on `Node.prototype`: parent/child/
//! sibling links, `nodeType`/`nodeName`/`tagName`/`localName`/
//! `namespaceURI`/`isConnected`/`ownerDocument`.

use std::ffi::CString;

use quickjs_sys as sys;

use super::collections::node_list;
use super::node_registry::{dom_opaque, node_class_id_for, node_id, node_object};
use super::util::{new_js_string, Getter};

unsafe fn navigation_node(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  relation: impl FnOnce(&dom::Dom, dom::NodeId) -> Option<dom::NodeId>,
) -> sys::JSValue {
  let Some(id) = node_id(ctx, this_val) else {
    return sys::js_null();
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() {
    return sys::js_null();
  }
  relation(&*dom, id)
    .map(|id| {
      let class_id = node_class_id_for(ctx, dom, id);
      node_object(ctx, class_id, id)
    })
    .unwrap_or_else(sys::js_null)
}

unsafe extern "C" fn node_parent_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  navigation_node(ctx, this_val, |dom, id| {
    dom.get(id).and_then(|node| node.parent)
  })
}

unsafe extern "C" fn node_first_child_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  navigation_node(ctx, this_val, |dom, id| {
    dom.get(id).and_then(|node| node.children.first().copied())
  })
}

unsafe extern "C" fn node_last_child_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  navigation_node(ctx, this_val, |dom, id| {
    dom.get(id).and_then(|node| node.children.last().copied())
  })
}

unsafe fn sibling(ctx: *mut sys::JSContext, this_val: sys::JSValue, next: bool) -> sys::JSValue {
  navigation_node(ctx, this_val, |dom, id| {
    let parent = dom.get(id)?.parent?;
    let siblings = &dom.get(parent)?.children;
    let index = siblings.iter().position(|&candidate| candidate == id)?;
    if next {
      siblings.get(index + 1).copied()
    } else {
      index
        .checked_sub(1)
        .and_then(|index| siblings.get(index).copied())
    }
  })
}

unsafe extern "C" fn node_previous_sibling_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  sibling(ctx, this_val, false)
}
unsafe extern "C" fn node_next_sibling_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  sibling(ctx, this_val, true)
}

unsafe extern "C" fn node_children_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = node_id(ctx, this_val) else {
    return sys::JS_NewArray(ctx);
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() {
    return sys::JS_NewArray(ctx);
  }
  let children = (*dom)
    .get(id)
    .map(|node| node.children.clone())
    .unwrap_or_default();
  node_list(ctx, children)
}

unsafe extern "C" fn node_type_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = node_id(ctx, this_val) else {
    return sys::js_undefined();
  };
  let dom = dom_opaque(ctx);
  let value = if dom.is_null() {
    None
  } else {
    (*dom).get(id).map(|node| match node.data {
      dom::NodeData::Document => 9.0,
      dom::NodeData::Element { .. } => 1.0,
      dom::NodeData::Text(_) => 3.0,
      dom::NodeData::Comment(_) => 8.0,
      dom::NodeData::DocumentFragment => 11.0,
    })
  };
  value.map(sys::js_float64).unwrap_or_else(sys::js_undefined)
}

unsafe extern "C" fn node_name_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = node_id(ctx, this_val) else {
    return sys::js_undefined();
  };
  let dom = dom_opaque(ctx);
  let name = if dom.is_null() {
    None
  } else {
    (*dom).get(id).map(|node| match &node.data {
      dom::NodeData::Document => "#document".to_owned(),
      dom::NodeData::Element { tag, .. } => tag.to_ascii_uppercase(),
      dom::NodeData::Text(_) => "#text".to_owned(),
      dom::NodeData::Comment(_) => "#comment".to_owned(),
      dom::NodeData::DocumentFragment => "#document-fragment".to_owned(),
    })
  };
  name
    .map(|name| new_js_string(ctx, &name))
    .unwrap_or_else(sys::js_undefined)
}

/// Real (Element-only, per spec) `tagName` — uppercased, matching how a
/// real browser reports an HTML element's tag name regardless of source
/// case. Exposed generically on this engine's one `Node` class rather than
/// a real `Element` subclass (documented deviation, same as every other
/// Element-only property this file already exposes on `Node.prototype`,
/// e.g. `checked`/`href`): `undefined` for a non-`Element` node, matching
/// `value`'s own "generic host, permissive off-type result" convention.
unsafe extern "C" fn node_tag_name_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = node_id(ctx, this_val) else {
    return sys::js_undefined();
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() {
    return sys::js_undefined();
  }
  match (*dom).get(id).map(|node| &node.data) {
    Some(dom::NodeData::Element { tag, .. }) => new_js_string(ctx, &tag.to_ascii_uppercase()),
    _ => sys::js_undefined(),
  }
}

/// Real (Element-only) `localName` — the tag exactly as stored, no case
/// change. This engine stores a tag's case as given at creation time
/// (`document.createElement`) or already-lowercased by `html5ever`
/// (parsed markup), so unlike `tagName` there's no forced uppercasing.
unsafe extern "C" fn node_local_name_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = node_id(ctx, this_val) else {
    return sys::js_undefined();
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() {
    return sys::js_undefined();
  }
  match (*dom).get(id).map(|node| &node.data) {
    Some(dom::NodeData::Element { tag, .. }) => new_js_string(ctx, tag),
    _ => sys::js_undefined(),
  }
}

/// Real (Element-only) `namespaceURI`. This engine never parses or creates
/// SVG/MathML foreign content (`html::sink`'s own documented scope cut —
/// no foreign-content handling exists), so every `Element` it can produce
/// is genuinely in the HTML namespace; `null` for anything else, matching
/// a real browser's `Node.namespaceURI` for non-`Element` node kinds.
unsafe extern "C" fn node_namespace_uri_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = node_id(ctx, this_val) else {
    return sys::js_null();
  };
  let dom = dom_opaque(ctx);
  if dom.is_null() {
    return sys::js_null();
  }
  match (*dom).get(id).map(|node| &node.data) {
    Some(dom::NodeData::Element { .. }) => new_js_string(ctx, "http://www.w3.org/1999/xhtml"),
    _ => sys::js_null(),
  }
}

/// Real `Node.prototype.isConnected`: whether this node is still attached
/// to the document tree. `false` for a stale/detached node or a `this`
/// that isn't a node at all — same permissive style every other read-only
/// accessor here uses for a malformed `this`.
unsafe extern "C" fn node_is_connected_get(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
) -> sys::JSValue {
  let Some(id) = node_id(ctx, this_val) else {
    return sys::js_bool(false);
  };
  let dom = dom_opaque(ctx);
  sys::js_bool(!dom.is_null() && (*dom).is_connected(id))
}

/// Real `Node.prototype.ownerDocument`: this engine has exactly one
/// `Document` per context (no iframes, see `window.rs`'s own doc comment),
/// so every node's owner is the same shared `document` object every other
/// binding in this crate (`window`, `location`, ...) already hands back —
/// not a `Node`-class wrapper around the DOM's own root, which would be a
/// different, unrelated object identity from `document.getElementById`
/// callers already hold.
unsafe extern "C" fn node_owner_document_get(
  ctx: *mut sys::JSContext,
  _this_val: sys::JSValue,
) -> sys::JSValue {
  crate::document::get_or_create(ctx)
}

pub(super) unsafe fn define_navigation(ctx: *mut sys::JSContext, proto: sys::JSValue) {
  for (name, getter) in [
    ("parentNode", node_parent_get as Getter),
    ("firstChild", node_first_child_get as Getter),
    ("lastChild", node_last_child_get as Getter),
    ("previousSibling", node_previous_sibling_get as Getter),
    ("nextSibling", node_next_sibling_get as Getter),
    ("childNodes", node_children_get as Getter),
    ("nodeType", node_type_get as Getter),
    ("nodeName", node_name_get as Getter),
    ("tagName", node_tag_name_get as Getter),
    ("localName", node_local_name_get as Getter),
    ("namespaceURI", node_namespace_uri_get as Getter),
    ("isConnected", node_is_connected_get as Getter),
    ("ownerDocument", node_owner_document_get as Getter),
  ] {
    let name = CString::new(name).unwrap();
    let getter = sys::JS_NewCFunction2(
      ctx,
      std::mem::transmute::<Getter, sys::JSCFunction>(getter),
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
}
