//! DOM↔JS bindings: a `Node` JS class wrapping a boxed `dom::NodeId` as its
//! opaque data, plus `document.getElementById(id)` and the `textContent`
//! accessor on `Node.prototype`. Backed by a `dom::Dom` stashed in the
//! context's opaque slot (see `Context::with_dom`).
//!
//! Real per-`dom::NodeId` object identity now (`get_or_create_node_object`,
//! `NODE_OBJECTS` below) — a genuine bug this fixes, not a new feature:
//! `document.getElementById(id)` used to build a brand-new `Node` JS
//! object on every single call, so `el.addEventListener(...)` followed by
//! a *separate* `document.getElementById(id)` call later (any real page
//! calling it twice — extremely common; a listener attached at page-load
//! time and dispatched from a later, separate `eval()`, e.g. a real
//! coordinate click routed through `profile-worker`) landed the listener
//! on a wrapper object that was never seen again, since
//! `events.rs`'s `__listeners` storage lives as an own property on the
//! wrapper instance, not keyed by `NodeId` itself. Caching one JS object
//! per real `NodeId` (thread-local, keyed by `JSContext` pointer then
//! `NodeId` — same convention as `timers`/`fetch_async`'s own registries)
//! gives every lookup of the same DOM node the same JS object identity, so
//! its properties (listeners included) actually persist the way a real
//! `Node`'s object identity does.
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

const MAX_SELECTOR_LENGTH: usize = 1024;
const MAX_SELECTOR_VISITS: usize = 4096;
const MAX_SELECTOR_RESULTS: usize = 2048;
const MAX_TAG_NAME_LENGTH: usize = 64;
const MAX_ATTRIBUTE_NAME_LENGTH: usize = 64;
const MAX_ATTRIBUTE_VALUE_LENGTH: usize = 4096;
const MAX_TEXT_NODE_LENGTH: usize = 16 * 1024;

thread_local! {
    static NODE_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
}

/// Returns a real, identity-stable `Node` JS object for `id` — the same
/// `JSValue` every prior call for this `(ctx, id)` pair returned, not a
/// fresh one. Each call hands back a new *owned reference* to that same
/// object (`JS_DupValue`), matching every other binding in this crate's
/// own "caller owns what it gets back" convention — the cache itself
/// holds one reference for as long as the `Context` lives, freed by
/// [`cleanup`].
pub(crate) unsafe fn node_object(
    ctx: *mut sys::JSContext,
    class_id: sys::JSClassID,
    id: dom::NodeId,
) -> sys::JSValue {
    let ctx_key = ctx as usize;
    let cached = NODE_OBJECTS.with(|reg| {
        reg.borrow()
            .get(&ctx_key)
            .and_then(|nodes| nodes.get(&id).copied())
    });
    if let Some(obj) = cached {
        return sys::JS_DupValue(ctx, obj);
    }

    let obj = make_node_object(ctx, class_id, id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    NODE_OBJECTS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx_key)
            .or_default()
            .insert(id, sys::JS_DupValue(ctx, obj));
    });
    obj
}

/// Returns the DOM parent of a node, if the context still has live DOM
/// state. This deliberately exposes IDs rather than DOM references so event
/// dispatch cannot retain or mutate the DOM while walking its propagation path.
pub(crate) unsafe fn parent_node_id(
    ctx: *mut sys::JSContext,
    id: dom::NodeId,
) -> Option<dom::NodeId> {
    let dom = dom_opaque(ctx);
    (!dom.is_null())
        .then(|| (*dom).get(id).and_then(|node| node.parent))
        .flatten()
}

unsafe fn evict_node_object(ctx: *mut sys::JSContext, id: dom::NodeId) {
    let cached = NODE_OBJECTS.with(|reg| {
        reg.borrow_mut()
            .get_mut(&(ctx as usize))
            .and_then(|nodes| nodes.remove(&id))
    });
    if let Some(object) = cached {
        sys::JS_FreeValue(ctx, object);
    }
}

/// Frees every cached `Node` object for `ctx` — must run before
/// `JS_FreeContext` (same ordering requirement `timers::cleanup`/
/// `fetch_async::cleanup` already document), since a `JSValue` can't be
/// freed against an already-freed context.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    if let Some(nodes) = NODE_OBJECTS.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
        for (_, obj) in nodes {
            sys::JS_FreeValue(ctx, obj);
        }
    }
}

/// See `crate::class_registry` - one registry entry per `JSRuntime`, not
/// a single value shared across every `Runtime` in the process.
const NODE_CLASS_KIND: &str = "Node";

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
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

unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe fn throw_type_error(ctx: *mut sys::JSContext, message: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_js_string(ctx, message))
}

unsafe fn node_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut dom::NodeId {
    let class_id = crate::class_registry::class_id_for(rt, NODE_CLASS_KIND);
    sys::JS_GetOpaque(this_val, class_id) as *mut dom::NodeId
}

pub(crate) unsafe fn node_id(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> Option<dom::NodeId> {
    let ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    (!ptr.is_null()).then(|| *ptr)
}

unsafe fn dom_opaque(ctx: *mut sys::JSContext) -> *mut dom::Dom {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return std::ptr::null_mut();
    }
    // A raw pointer to a field within the boxed `HostState` - sound as
    // long as the box isn't moved, which `Context` guarantees the same
    // way it already did when this pointed straight at a boxed `dom::Dom`
    // (see `Context`'s own doc comment on why moving the `Box` doesn't
    // move the heap allocation it points to).
    std::ptr::addr_of_mut!((*state).dom)
}

fn element_snapshot(dom: &dom::Dom, id: dom::NodeId) -> Option<css::ElementSnapshot> {
    let node = dom.get(id)?;
    let dom::NodeData::Element {
        tag, attributes, ..
    } = &node.data
    else {
        return None;
    };
    Some(css::ElementSnapshot {
        tag: tag.clone(),
        id: attributes.get("id").cloned(),
        classes: attributes
            .get("class")
            .map(|value| value.split_ascii_whitespace().map(str::to_owned).collect())
            .unwrap_or_default(),
        attributes: attributes
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        preceding_siblings: Vec::new(),
        has_following_sibling: false,
    })
}

/// Builds the finite left-sibling chain needed by `+`/`~` matching. A
/// sibling snapshot never recursively includes following siblings, avoiding
/// a `first -> second -> first` cycle while retaining `a + b + c` support.
fn preceding_snapshot(dom: &dom::Dom, id: dom::NodeId) -> Option<css::ElementSnapshot> {
    let mut result = element_snapshot(dom, id)?;
    let parent = dom.get(id)?.parent?;
    let siblings = &dom.get(parent)?.children;
    let position = siblings.iter().position(|&sibling| sibling == id)?;
    result.preceding_siblings = siblings[..position]
        .iter()
        .filter_map(|&sibling| preceding_snapshot(dom, sibling))
        .collect();
    Some(result)
}

fn snapshot(dom: &dom::Dom, id: dom::NodeId) -> Option<css::ElementSnapshot> {
    let mut result = preceding_snapshot(dom, id)?;
    let parent = dom.get(id)?.parent?;
    let siblings = &dom.get(parent)?.children;
    let position = siblings.iter().position(|&sibling| sibling == id)?;
    result.has_following_sibling = siblings[position + 1..]
        .iter()
        .any(|&sibling| element_snapshot(dom, sibling).is_some());
    Some(result)
}

fn selector_chain(dom: &dom::Dom, id: dom::NodeId) -> Option<Vec<css::ElementSnapshot>> {
    let mut ids = Vec::new();
    let mut current = Some(id);
    while let Some(node_id) = current {
        if matches!(dom.get(node_id)?.data, dom::NodeData::Element { .. }) {
            ids.push(node_id);
        }
        current = dom.get(node_id)?.parent;
    }
    ids.reverse();
    ids.into_iter()
        .map(|node_id| snapshot(dom, node_id))
        .collect()
}

fn matching_nodes(
    dom: &dom::Dom,
    start: dom::NodeId,
    selector: &str,
    include_start: bool,
) -> Result<Vec<dom::NodeId>, &'static str> {
    if selector.is_empty() || selector.len() > MAX_SELECTOR_LENGTH {
        return Err("selector is empty or exceeds the maximum length");
    }
    let stylesheet = css::parse_stylesheet(&format!("{selector} {{}}"));
    let Some(rule) = stylesheet.rules.first() else {
        return Err("unsupported selector syntax");
    };
    if stylesheet.rules.len() != 1 || rule.selectors.0.is_empty() {
        return Err("unsupported selector syntax");
    }
    let mut result = Vec::new();
    let mut stack = vec![start];
    let mut visits = 0usize;
    while let Some(node_id) = stack.pop() {
        visits += 1;
        if visits > MAX_SELECTOR_VISITS {
            return Err("selector traversal limit exceeded");
        }
        let node = dom.get(node_id).ok_or("invalid DOM node")?;
        if (include_start || node_id != start) && matches!(node.data, dom::NodeData::Element { .. })
        {
            let chain = selector_chain(dom, node_id).ok_or("invalid DOM ancestry")?;
            if rule
                .selectors
                .0
                .iter()
                .any(|candidate| css::selector_matches(candidate, &chain))
            {
                result.push(node_id);
                if result.len() > MAX_SELECTOR_RESULTS {
                    return Err("selector result limit exceeded");
                }
            }
        }
        stack.extend(node.children.iter().rev().copied());
    }
    Ok(result)
}

unsafe fn query_selector_all(
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
    let array = sys::JS_NewArray(ctx);
    let class_id = crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND);
    for (index, node_id) in nodes.into_iter().enumerate() {
        sys::JS_SetPropertyUint32(
            ctx,
            array,
            index as u32,
            node_object(ctx, class_id, node_id),
        );
    }
    array
}

unsafe extern "C" fn node_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = node_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

type Getter =
    unsafe extern "C" fn(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue;
type Setter = unsafe extern "C" fn(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue;

unsafe extern "C" fn node_text_content_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if node_ptr.is_null() || dom_ptr.is_null() {
        return sys::js_undefined();
    }
    new_js_string(ctx, &(*dom_ptr).text_content(*node_ptr))
}

unsafe extern "C" fn node_text_content_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    let Some(text) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        (*dom_ptr).set_text_content(*node_ptr, &text);
    }
    sys::js_undefined()
}

/// Defines the `textContent` accessor on `proto`. Takes ownership of `proto`
/// only in the sense of mutating it in place — callers still own the value.
unsafe fn define_text_content(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("textContent").unwrap();

    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(node_text_content_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let setter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(node_text_content_set),
        name.as_ptr(),
        1,
        sys::JS_CFUNC_SETTER,
        0,
    );

    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
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

unsafe fn attribute_property_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    name: &str,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    new_js_string(ctx, (*dom).attribute(id, name).unwrap_or_default())
}

unsafe fn attribute_property_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
    name: &str,
) -> sys::JSValue {
    let Some(value) = read_js_string(ctx, val) else {
        return throw_type_error(ctx, "attribute value must be a string");
    };
    if value.len() > MAX_ATTRIBUTE_VALUE_LENGTH {
        return throw_type_error(ctx, "attribute value exceeds the maximum length");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).set_attribute(id, name, &value);
    sys::js_undefined()
}

unsafe extern "C" fn node_id_property_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "id")
}
unsafe extern "C" fn node_id_property_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "id")
}
unsafe extern "C" fn node_class_name_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "class")
}
unsafe extern "C" fn node_class_name_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "class")
}

unsafe fn define_attribute_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, getter, setter) in [
        (
            "id",
            node_id_property_get as Getter,
            node_id_property_set as Setter,
        ),
        (
            "className",
            node_class_name_get as Getter,
            node_class_name_set as Setter,
        ),
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
        let setter = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<Setter, sys::JSCFunction>(setter),
            name.as_ptr(),
            1,
            sys::JS_CFUNC_SETTER,
            0,
        );
        let atom = sys::JS_NewAtom(ctx, name.as_ptr());
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
}

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
            node_object(
                ctx,
                crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND),
                id,
            )
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
    let array = sys::JS_NewArray(ctx);
    let Some(id) = node_id(ctx, this_val) else {
        return array;
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return array;
    }
    let children = (*dom)
        .get(id)
        .map(|node| node.children.clone())
        .unwrap_or_default();
    let class_id = crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND);
    for (index, child) in children.into_iter().enumerate() {
        sys::JS_SetPropertyUint32(ctx, array, index as u32, node_object(ctx, class_id, child));
    }
    array
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
        })
    };
    name.map(|name| new_js_string(ctx, &name))
        .unwrap_or_else(sys::js_undefined)
}

unsafe fn define_navigation(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, getter) in [
        ("parentNode", node_parent_get as Getter),
        ("firstChild", node_first_child_get as Getter),
        ("lastChild", node_last_child_get as Getter),
        ("previousSibling", node_previous_sibling_get as Getter),
        ("nextSibling", node_next_sibling_get as Getter),
        ("childNodes", node_children_get as Getter),
        ("nodeType", node_type_get as Getter),
        ("nodeName", node_name_get as Getter),
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

unsafe extern "C" fn node_value_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if node_ptr.is_null() || dom_ptr.is_null() {
        return sys::js_undefined();
    }
    new_js_string(ctx, &(*dom_ptr).value(*node_ptr))
}

unsafe extern "C" fn node_value_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    let Some(text) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        (*dom_ptr).set_value(*node_ptr, &text);
        // Real `"input"` semantics: fires on every value mutation,
        // regardless of whether it came from a user keystroke
        // (`profile-worker`'s `type_key`) or a script setting `.value =`
        // directly — matches the real DOM, which doesn't distinguish the
        // two for this event.
        crate::events::dispatch(ctx, this_val, "input");
    }
    sys::js_undefined()
}

/// Defines the `value` accessor on `proto` — real, independent of
/// `textContent` (see `dom::Dom::value`/`set_value`'s own docs for the
/// deviation from a typed `HTMLInputElement`/`HTMLTextAreaElement`
/// hierarchy this generic `Node` class makes).
unsafe fn define_value(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("value").unwrap();

    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(node_value_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let setter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(node_value_set),
        name.as_ptr(),
        1,
        sys::JS_CFUNC_SETTER,
        0,
    );

    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
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
/// `document_active_element_get` below).
unsafe fn define_focus_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
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

fn valid_tag_name(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= MAX_TAG_NAME_LENGTH
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_ATTRIBUTE_NAME_LENGTH
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
}

fn is_ancestor(dom: &dom::Dom, ancestor: dom::NodeId, mut node: dom::NodeId) -> bool {
    loop {
        if node == ancestor {
            return true;
        }
        let Some(parent) = dom.get(node).and_then(|node| node.parent) else {
            return false;
        };
        node = parent;
    }
}

unsafe fn evict_subtree(ctx: *mut sys::JSContext, dom: &dom::Dom, id: dom::NodeId) {
    let children = dom
        .get(id)
        .map(|node| node.children.clone())
        .unwrap_or_default();
    for child in children {
        evict_subtree(ctx, dom, child);
    }
    evict_node_object(ctx, id);
}

unsafe extern "C" fn node_append_child(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "child node is required");
    }
    let Some(parent) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "append target must be a node");
    };
    let child_value = *argv;
    let Some(child) = node_id(ctx, child_value) else {
        return throw_type_error(ctx, "child must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(parent).is_none() || (*dom).get(child).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if is_ancestor(&*dom, child, parent) {
        return throw_type_error(ctx, "cannot append an ancestor into its descendant");
    }
    (*dom).append_child(parent, child);
    sys::JS_DupValue(ctx, child_value)
}

unsafe extern "C" fn node_get_attribute(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "attribute name is required");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "attribute name must be a string");
    };
    if !valid_attribute_name(&name) {
        return throw_type_error(ctx, "invalid attribute name");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_null();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    (*dom)
        .attribute(id, &name)
        .map(|value| new_js_string(ctx, value))
        .unwrap_or_else(sys::js_null)
}

unsafe extern "C" fn node_insert_before(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "new node and reference node are required");
    }
    let Some(parent) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "insert target must be a node");
    };
    let new_value = *argv;
    let reference_value = *argv.add(1);
    let (Some(new_node), Some(reference_node)) =
        (node_id(ctx, new_value), node_id(ctx, reference_value))
    else {
        return throw_type_error(ctx, "insert arguments must be nodes");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null()
        || (*dom).get(parent).is_none()
        || (*dom).get(new_node).is_none()
        || (*dom).get(reference_node).is_none()
    {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if (*dom).get(reference_node).and_then(|node| node.parent) != Some(parent) {
        return throw_type_error(ctx, "reference node is not a child of this parent");
    }
    if is_ancestor(&*dom, new_node, parent) {
        return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
    }
    (*dom).insert_before(reference_node, new_node);
    sys::JS_DupValue(ctx, new_value)
}

unsafe extern "C" fn node_set_attribute(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "attribute name and value are required");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "attribute name must be a string");
    };
    let Some(value) = read_js_string(ctx, *argv.add(1)) else {
        return throw_type_error(ctx, "attribute value must be a string");
    };
    if !valid_attribute_name(&name) || value.len() > MAX_ATTRIBUTE_VALUE_LENGTH {
        return throw_type_error(ctx, "invalid attribute");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).set_attribute(id, &name, &value);
    sys::js_undefined()
}

unsafe extern "C" fn node_remove_attribute(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "attribute name is required");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "attribute name must be a string");
    };
    if !valid_attribute_name(&name) {
        return throw_type_error(ctx, "invalid attribute name");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).remove_attribute(id, &name);
    sys::js_undefined()
}

unsafe extern "C" fn node_remove(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if !dom.is_null() && (*dom).get(id).is_some() {
        if id == (*dom).root() {
            return throw_type_error(ctx, "document root cannot be removed");
        }
        evict_subtree(ctx, &*dom, id);
        (*dom).remove(id);
    }
    sys::js_undefined()
}

unsafe extern "C" fn node_remove_child(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "child node is required");
    }
    let Some(parent) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "remove target must be a node");
    };
    let child_value = *argv;
    let Some(child) = node_id(ctx, child_value) else {
        return throw_type_error(ctx, "child must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(parent).is_none() || (*dom).get(child).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if (*dom).get(child).and_then(|node| node.parent) != Some(parent) {
        return throw_type_error(ctx, "node is not a child of this parent");
    }
    evict_subtree(ctx, &*dom, child);
    (*dom).remove(child);
    sys::JS_DupValue(ctx, child_value)
}

unsafe fn define_mutation_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, function, arity) in [
        ("appendChild", node_append_child as sys::JSCFunction, 1),
        ("getAttribute", node_get_attribute as sys::JSCFunction, 1),
        ("setAttribute", node_set_attribute as sys::JSCFunction, 2),
        (
            "removeAttribute",
            node_remove_attribute as sys::JSCFunction,
            1,
        ),
        ("insertBefore", node_insert_before as sys::JSCFunction, 2),
        ("removeChild", node_remove_child as sys::JSCFunction, 1),
        ("remove", node_remove as sys::JSCFunction, 0),
    ] {
        let name = CString::new(name).unwrap();
        let value = sys::JS_NewCFunction2(
            ctx,
            function,
            name.as_ptr(),
            arity,
            sys::JS_CFUNC_GENERIC,
            0,
        );
        sys::JS_SetPropertyStr(ctx, proto, name.as_ptr(), value);
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

fn node_matches_selector(
    dom: &dom::Dom,
    id: dom::NodeId,
    selector: &str,
) -> Result<bool, &'static str> {
    if selector.is_empty() || selector.len() > MAX_SELECTOR_LENGTH {
        return Err("selector is empty or exceeds the maximum length");
    }
    if !matches!(
        dom.get(id).map(|node| &node.data),
        Some(dom::NodeData::Element { .. })
    ) {
        return Ok(false);
    }
    let stylesheet = css::parse_stylesheet(&format!("{selector} {{}}"));
    let Some(rule) = stylesheet.rules.first() else {
        return Err("unsupported selector syntax");
    };
    if stylesheet.rules.len() != 1 || rule.selectors.0.is_empty() {
        return Err("unsupported selector syntax");
    }
    let chain = selector_chain(dom, id).ok_or("invalid DOM ancestry")?;
    Ok(rule
        .selectors
        .0
        .iter()
        .any(|candidate| css::selector_matches(candidate, &chain)))
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
                return node_object(
                    ctx,
                    crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND),
                    current,
                )
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

unsafe fn define_selector_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, function) in [
        ("querySelector", node_query_selector as sys::JSCFunction),
        (
            "querySelectorAll",
            node_query_selector_all as sys::JSCFunction,
        ),
        ("matches", node_matches as sys::JSCFunction),
        ("closest", node_closest as sys::JSCFunction),
    ] {
        let name = CString::new(name).unwrap();
        let value =
            sys::JS_NewCFunction2(ctx, function, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, proto, name.as_ptr(), value);
    }
}

/// Registers the `Node` class on `ctx`'s runtime (if not already done for
/// this runtime) and builds this context's `Node.prototype`.
unsafe fn ensure_node_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("Node").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(node_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, NODE_CLASS_KIND, &def);

    let proto = sys::JS_NewObject(ctx);
    define_text_content(ctx, proto);
    define_attribute_properties(ctx, proto);
    define_navigation(ctx, proto);
    define_value(ctx, proto);
    define_focus_methods(ctx, proto);
    define_mutation_methods(ctx, proto);
    define_selector_methods(ctx, proto);
    crate::events::define_event_target(ctx, proto);
    sys::JS_SetClassProto(ctx, class_id, proto);

    class_id
}

unsafe fn make_node_object(
    ctx: *mut sys::JSContext,
    class_id: sys::JSClassID,
    id: dom::NodeId,
) -> sys::JSValue {
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    sys::JS_SetOpaque(obj, Box::into_raw(Box::new(id)) as *mut c_void);
    obj
}

unsafe extern "C" fn document_get_element_by_id(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_null();
    }
    let Some(id) = read_js_string(ctx, *argv) else {
        return sys::js_null();
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::js_null();
    }
    match (*dom_ptr).find_by_id(&id) {
        Some(node_id) => node_object(
            ctx,
            crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND),
            node_id,
        ),
        None => sys::js_null(),
    }
}

unsafe extern "C" fn document_create_element(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "tag name is required");
    }
    let Some(tag) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "tag name must be a string");
    };
    if !valid_tag_name(&tag) {
        return throw_type_error(ctx, "invalid tag name");
    }
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "document is unavailable");
    }
    let id = (*dom).create_element(&tag);
    node_object(
        ctx,
        crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND),
        id,
    )
}

unsafe extern "C" fn document_create_text_node(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "text content is required");
    }
    let Some(text) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "text content must be a string");
    };
    if text.len() > MAX_TEXT_NODE_LENGTH {
        return throw_type_error(ctx, "text node exceeds the maximum length");
    }
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "document is unavailable");
    }
    let id = (*dom).create_text(&text);
    node_object(
        ctx,
        crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND),
        id,
    )
}

unsafe extern "C" fn document_query_selector(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "selector is required");
    }
    let Some(selector) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "selector must be a string");
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::js_null();
    }
    let all = query_selector_all(ctx, (*dom_ptr).root(), &selector, true);
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

unsafe extern "C" fn document_query_selector_all(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "selector is required");
    }
    let Some(selector) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "selector must be a string");
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::JS_NewArray(ctx);
    }
    query_selector_all(ctx, (*dom_ptr).root(), &selector, true)
}

unsafe extern "C" fn document_active_element_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::js_null();
    }
    match (*dom_ptr).active_element() {
        Some(node_id) => node_object(
            ctx,
            crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND),
            node_id,
        ),
        None => sys::js_null(),
    }
}

unsafe extern "C" fn document_body_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    match matching_nodes(&*dom, (*dom).root(), "body", true) {
        Ok(nodes) => nodes
            .into_iter()
            .next()
            .map(|id| {
                node_object(
                    ctx,
                    crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND),
                    id,
                )
            })
            .unwrap_or_else(sys::js_null),
        Err(_) => sys::js_null(),
    }
}

unsafe extern "C" fn document_document_element_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    match matching_nodes(&*dom, (*dom).root(), "html", true) {
        Ok(nodes) => nodes
            .into_iter()
            .next()
            .map(|id| {
                node_object(
                    ctx,
                    crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND),
                    id,
                )
            })
            .unwrap_or_else(sys::js_null),
        Err(_) => sys::js_null(),
    }
}

/// Defines the real, read-only `document.activeElement` getter — backed by
/// `dom::Dom`'s own focus state, so it reflects whatever the most recent
/// `.focus()`/`.blur()` call (JS-driven or `profile-worker`'s coordinate
/// click routing) left focused.
unsafe fn define_active_element(ctx: *mut sys::JSContext, document: sys::JSValue) {
    let name = CString::new("activeElement").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(document_active_element_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        document,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}

unsafe fn define_body(ctx: *mut sys::JSContext, document: sys::JSValue) {
    let name = CString::new("body").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(document_body_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        document,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}

unsafe fn define_document_element(ctx: *mut sys::JSContext, document: sys::JSValue) {
    let name = CString::new("documentElement").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(document_document_element_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        document,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}

/// Registers the `Node` class and a global `document` object exposing
/// `getElementById(id)`/`activeElement`. Callers must have already pointed
/// the context's opaque slot at a live `dom::Dom` via `JS_SetContextOpaque`
/// — bindings read it back on every call and no-op (return null/undefined)
/// if unset.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    ensure_node_class(ctx);
    crate::events::register(ctx);

    let document = crate::document::get_or_create(ctx);

    let name = CString::new("getElementById").unwrap();
    let get_by_id = sys::JS_NewCFunction2(
        ctx,
        document_get_element_by_id,
        name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, document, name.as_ptr(), get_by_id);
    let name = CString::new("createElement").unwrap();
    let create_element = sys::JS_NewCFunction2(
        ctx,
        document_create_element,
        name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, document, name.as_ptr(), create_element);
    let name = CString::new("createTextNode").unwrap();
    let create_text_node = sys::JS_NewCFunction2(
        ctx,
        document_create_text_node,
        name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, document, name.as_ptr(), create_text_node);
    for (name, function) in [
        ("querySelector", document_query_selector as sys::JSCFunction),
        (
            "querySelectorAll",
            document_query_selector_all as sys::JSCFunction,
        ),
    ] {
        let name = CString::new(name).unwrap();
        let value =
            sys::JS_NewCFunction2(ctx, function, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, document, name.as_ptr(), value);
    }
    define_active_element(ctx, document);
    define_body(ctx, document);
    define_document_element(ctx, document);

    sys::JS_FreeValue(ctx, document);
}
