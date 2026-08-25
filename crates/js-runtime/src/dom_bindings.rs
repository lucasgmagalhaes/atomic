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
/// Bounds `innerHTML`/`outerHTML` input — real HTML parsing has no natural
/// selector-style traversal cap to reuse, so the input string length is the
/// bound (same "cap the untrusted string, not the derived work" approach
/// `MAX_ATTRIBUTE_VALUE_LENGTH`/`MAX_TEXT_NODE_LENGTH` already take).
const MAX_HTML_LENGTH: usize = 64 * 1024;
/// Bounds the variadic node/string argument list `prepend`/`append`/
/// `before`/`after`/`replaceWith` each accept — an unbounded arg count from
/// a script would otherwise let one call insert an unbounded number of
/// children in one shot, the same class of concern every other bound in
/// this file guards against.
const MAX_CHILD_NODE_ARGS: usize = 256;

thread_local! {
    static NODE_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
    static CLASS_LIST_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
    /// Last synced token count per `(ctx, NodeId)` classList object, so
    /// `sync_class_list` knows how many trailing indexed properties (from a
    /// shrunk class attribute) need clearing to `undefined` rather than left
    /// stale from a longer previous token list.
    static CLASS_LIST_LENGTHS: RefCell<HashMap<usize, HashMap<dom::NodeId, usize>>> = RefCell::new(HashMap::new());
    static DATASET_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
    /// Last synced camelCase key set per `(ctx, NodeId)` dataset object, so
    /// `sync_dataset` can clear a key whose backing `data-*` attribute was
    /// removed since the last sync (same "diff against what was there
    /// before" need `CLASS_LIST_LENGTHS` has, just keyed by name instead of
    /// a plain trailing count since dataset keys aren't densely indexed).
    static DATASET_KEYS: RefCell<HashMap<usize, HashMap<dom::NodeId, Vec<String>>>> = RefCell::new(HashMap::new());
    static ATTRS_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
    static ATTRS_LENGTHS: RefCell<HashMap<usize, HashMap<dom::NodeId, usize>>> = RefCell::new(HashMap::new());
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
    let cached = CLASS_LIST_OBJECTS.with(|reg| {
        reg.borrow_mut()
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
    let cached = DATASET_OBJECTS.with(|reg| {
        reg.borrow_mut()
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
    let cached = ATTRS_OBJECTS.with(|reg| {
        reg.borrow_mut()
            .get_mut(&(ctx as usize))
            .and_then(|nodes| nodes.remove(&id))
    });
    if let Some(object) = cached {
        sys::JS_FreeValue(ctx, object);
    }
    ATTRS_LENGTHS.with(|reg| {
        if let Some(nodes) = reg.borrow_mut().get_mut(&(ctx as usize)) {
            nodes.remove(&id);
        }
    });
    crate::css_style::evict(ctx, id);
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
    if let Some(objects) = CLASS_LIST_OBJECTS.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
        for (_, obj) in objects {
            sys::JS_FreeValue(ctx, obj);
        }
    }
    CLASS_LIST_LENGTHS.with(|reg| {
        reg.borrow_mut().remove(&(ctx as usize));
    });
    if let Some(objects) = DATASET_OBJECTS.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
        for (_, obj) in objects {
            sys::JS_FreeValue(ctx, obj);
        }
    }
    DATASET_KEYS.with(|reg| {
        reg.borrow_mut().remove(&(ctx as usize));
    });
    if let Some(objects) = ATTRS_OBJECTS.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
        for (_, obj) in objects {
            sys::JS_FreeValue(ctx, obj);
        }
    }
    ATTRS_LENGTHS.with(|reg| {
        reg.borrow_mut().remove(&(ctx as usize));
    });
    crate::css_style::cleanup(ctx);
}

/// See `crate::class_registry` - one registry entry per `JSRuntime`, not
/// a single value shared across every `Runtime` in the process.
const NODE_CLASS_KIND: &str = "Node";
const ELEMENT_CLASS_KIND: &str = "Element";
const HTML_ELEMENT_CLASS_KIND: &str = "HTMLElement";
const HTML_INPUT_CLASS_KIND: &str = "HTMLInputElement";
const HTML_BUTTON_CLASS_KIND: &str = "HTMLButtonElement";
const HTML_ANCHOR_CLASS_KIND: &str = "HTMLAnchorElement";
const HTML_IMAGE_CLASS_KIND: &str = "HTMLImageElement";
const HTML_CANVAS_CLASS_KIND: &str = "HTMLCanvasElement";
const HTML_FORM_CLASS_KIND: &str = "HTMLFormElement";
const HTML_SELECT_CLASS_KIND: &str = "HTMLSelectElement";

const ALL_NODE_CLASS_KINDS: &[&str] = &[
    NODE_CLASS_KIND,
    ELEMENT_CLASS_KIND,
    HTML_ELEMENT_CLASS_KIND,
    HTML_INPUT_CLASS_KIND,
    HTML_BUTTON_CLASS_KIND,
    HTML_ANCHOR_CLASS_KIND,
    HTML_IMAGE_CLASS_KIND,
    HTML_CANVAS_CLASS_KIND,
    HTML_FORM_CLASS_KIND,
    HTML_SELECT_CLASS_KIND,
];

/// Returns the class ID for a node based on its `NodeData` variant and tag.
/// Elements get their specific subclass (HTMLInputElement, etc.), text/comment/
/// document-fragment nodes get the base `Node` class.
unsafe fn node_class_id_for(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    if dom.is_null() {
        return crate::class_registry::class_id_for(rt, NODE_CLASS_KIND);
    }
    match (*dom).get(id).map(|n| &n.data) {
        Some(dom::NodeData::Element { tag, .. }) => {
            let kind = match tag.as_str() {
                "input" => HTML_INPUT_CLASS_KIND,
                "button" => HTML_BUTTON_CLASS_KIND,
                "a" => HTML_ANCHOR_CLASS_KIND,
                "img" => HTML_IMAGE_CLASS_KIND,
                "canvas" => HTML_CANVAS_CLASS_KIND,
                "form" => HTML_FORM_CLASS_KIND,
                "select" => HTML_SELECT_CLASS_KIND,
                _ => HTML_ELEMENT_CLASS_KIND,
            };
            let id = crate::class_registry::class_id_for(rt, kind);
            if id != 0 {
                id
            } else {
                let id = crate::class_registry::class_id_for(rt, ELEMENT_CLASS_KIND);
                if id != 0 {
                    id
                } else {
                    crate::class_registry::class_id_for(rt, NODE_CLASS_KIND)
                }
            }
        }
        _ => crate::class_registry::class_id_for(rt, NODE_CLASS_KIND),
    }
}

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
    for &kind in ALL_NODE_CLASS_KINDS {
        let class_id = crate::class_registry::class_id_for(rt, kind);
        if class_id == 0 {
            continue;
        }
        let ptr = sys::JS_GetOpaque(this_val, class_id) as *mut dom::NodeId;
        if !ptr.is_null() {
            return ptr;
        }
    }
    std::ptr::null_mut()
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

/// `document`/`Node.prototype`'s `getElementsByTagName`, matched directly
/// against each `Element`'s stored tag rather than routed through the CSS
/// selector engine (`matching_nodes`) — a plain string compare, so a tag
/// argument containing selector metacharacters (`.`, `#`, `,`, ...) can
/// never be reinterpreted as different selector syntax. `"*"` matches every
/// element, mirroring the real API's universal case. Case-sensitive, same
/// as this engine's CSS type-selector matching (`cascade.rs`'s
/// `SimpleSelector::Type`) — a real browser's HTML-document case
/// insensitivity isn't modeled here, consistent with that existing gap.
fn matching_by_tag(
    dom: &dom::Dom,
    start: dom::NodeId,
    tag: &str,
    include_start: bool,
) -> Result<Vec<dom::NodeId>, &'static str> {
    let mut result = Vec::new();
    let mut stack = vec![start];
    let mut visits = 0usize;
    while let Some(node_id) = stack.pop() {
        visits += 1;
        if visits > MAX_SELECTOR_VISITS {
            return Err("traversal limit exceeded");
        }
        let node = dom.get(node_id).ok_or("invalid DOM node")?;
        if include_start || node_id != start {
            if let dom::NodeData::Element { tag: node_tag, .. } = &node.data {
                if tag == "*" || node_tag == tag {
                    result.push(node_id);
                    if result.len() > MAX_SELECTOR_RESULTS {
                        return Err("result limit exceeded");
                    }
                }
            }
        }
        stack.extend(node.children.iter().rev().copied());
    }
    Ok(result)
}

/// `document`/`Node.prototype`'s `getElementsByClassName`, matched by real
/// token-set membership (every whitespace-separated token in `class_name`
/// must be present in an element's own `class` attribute tokens) — the
/// real spec algorithm, not a CSS class-selector compound built and run
/// through the selector engine, which would let a class name containing a
/// selector metacharacter (a literal `.`/`#`/`,` in an authored class
/// token, valid HTML even if unusual) be misinterpreted as more selector
/// syntax instead of one literal token.
fn matching_by_class(
    dom: &dom::Dom,
    start: dom::NodeId,
    class_name: &str,
    include_start: bool,
) -> Result<Vec<dom::NodeId>, &'static str> {
    let wanted = class_tokens(class_name);
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    let mut stack = vec![start];
    let mut visits = 0usize;
    while let Some(node_id) = stack.pop() {
        visits += 1;
        if visits > MAX_SELECTOR_VISITS {
            return Err("traversal limit exceeded");
        }
        let node = dom.get(node_id).ok_or("invalid DOM node")?;
        if include_start || node_id != start {
            if let dom::NodeData::Element { attributes, .. } = &node.data {
                let have = class_tokens(attributes.get("class").map(String::as_str).unwrap_or(""));
                if wanted.iter().all(|token| have.contains(token)) {
                    result.push(node_id);
                    if result.len() > MAX_SELECTOR_RESULTS {
                        return Err("result limit exceeded");
                    }
                }
            }
        }
        stack.extend(node.children.iter().rev().copied());
    }
    Ok(result)
}

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
unsafe fn html_collection(ctx: *mut sys::JSContext, nodes: Vec<dom::NodeId>) -> sys::JSValue {
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
unsafe fn node_list(ctx: *mut sys::JSContext, nodes: Vec<dom::NodeId>) -> sys::JSValue {
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
    node_list(ctx, nodes)
}

unsafe fn elements_by_tag_name(
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

unsafe fn elements_by_class_name(
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
unsafe extern "C" fn node_form_name_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "name")
}
unsafe extern "C" fn node_form_name_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "name")
}
unsafe extern "C" fn node_type_attribute_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "type")
}
unsafe extern "C" fn node_type_attribute_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "type")
}
unsafe extern "C" fn node_href_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "href")
}
unsafe extern "C" fn node_href_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "href")
}
unsafe extern "C" fn node_html_for_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "for")
}
unsafe extern "C" fn node_html_for_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "for")
}

/// Boolean-attribute reflection (`checked`/`disabled`/`selected`): HTML
/// boolean attributes are presence-only — any attribute value (including
/// `""`) means `true`, and `false` means the attribute is absent entirely,
/// not present with a falsy string value. Mirrors `attribute_property_get`/
/// `_set`'s structure but with that different get/set semantics.
unsafe fn boolean_attribute_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    name: &str,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    sys::js_bool(!dom.is_null() && (*dom).attribute(id, name).is_some())
}

unsafe fn boolean_attribute_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
    name: &str,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if sys::JS_ToBool(ctx, val) != 0 {
        (*dom).set_attribute(id, name, "");
    } else {
        (*dom).remove_attribute(id, name);
    }
    sys::js_undefined()
}

unsafe extern "C" fn node_checked_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_get(ctx, this_val, "checked")
}
unsafe extern "C" fn node_checked_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_set(ctx, this_val, val, "checked")
}
unsafe extern "C" fn node_disabled_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_get(ctx, this_val, "disabled")
}
unsafe extern "C" fn node_disabled_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_set(ctx, this_val, val, "disabled")
}
unsafe extern "C" fn node_selected_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_get(ctx, this_val, "selected")
}
unsafe extern "C" fn node_selected_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_set(ctx, this_val, val, "selected")
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
        (
            "name",
            node_form_name_get as Getter,
            node_form_name_set as Setter,
        ),
        (
            "type",
            node_type_attribute_get as Getter,
            node_type_attribute_set as Setter,
        ),
        ("href", node_href_get as Getter, node_href_set as Setter),
        (
            "checked",
            node_checked_get as Getter,
            node_checked_set as Setter,
        ),
        (
            "disabled",
            node_disabled_get as Getter,
            node_disabled_set as Setter,
        ),
        (
            "selected",
            node_selected_get as Getter,
            node_selected_set as Setter,
        ),
        (
            "htmlFor",
            node_html_for_get as Getter,
            node_html_for_set as Setter,
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

unsafe fn class_list_owner(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<dom::NodeId> {
    let name = CString::new("__nimbleClassListOwner").unwrap();
    let owner = sys::JS_GetPropertyStr(ctx, value, name.as_ptr());
    let id = node_id(ctx, owner);
    sys::JS_FreeValue(ctx, owner);
    id
}

fn class_tokens(value: &str) -> Vec<String> {
    value.split_ascii_whitespace().map(str::to_owned).collect()
}

fn valid_class_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_ATTRIBUTE_NAME_LENGTH
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
            reg.borrow()
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
        reg.borrow_mut()
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
unsafe fn array_prototype(ctx: *mut sys::JSContext) -> sys::JSValue {
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
    if value.len() > MAX_ATTRIBUTE_VALUE_LENGTH {
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
    if value.len() > MAX_ATTRIBUTE_VALUE_LENGTH {
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
    if value.len() > MAX_ATTRIBUTE_VALUE_LENGTH {
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
    if value.len() > MAX_ATTRIBUTE_VALUE_LENGTH {
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
        reg.borrow()
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
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, sys::JS_DupValue(ctx, object))
    });
    object
}

unsafe fn define_class_list(ctx: *mut sys::JSContext, proto: sys::JSValue) {
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
        sys::JS_SetPropertyStr(ctx, object, key.as_ptr(), new_js_string(ctx, value));
        current.push(camel);
    }
    let stale = DATASET_KEYS.with(|reg| {
        reg.borrow()
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
        reg.borrow_mut()
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
        reg.borrow()
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
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, sys::JS_DupValue(ctx, object))
    });
    object
}

unsafe fn define_dataset(ctx: *mut sys::JSContext, proto: sys::JSValue) {
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

unsafe fn attrs_owner(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<dom::NodeId> {
    let name = CString::new("__nimbleAttributesOwner").unwrap();
    let owner = sys::JS_GetPropertyStr(ctx, value, name.as_ptr());
    let id = node_id(ctx, owner);
    sys::JS_FreeValue(ctx, owner);
    id
}

unsafe fn make_attr_entry(ctx: *mut sys::JSContext, name: &str, value: &str) -> sys::JSValue {
    let entry = sys::JS_NewObject(ctx);
    let name_key = CString::new("name").unwrap();
    sys::JS_SetPropertyStr(ctx, entry, name_key.as_ptr(), new_js_string(ctx, name));
    let value_key = CString::new("value").unwrap();
    sys::JS_SetPropertyStr(ctx, entry, value_key.as_ptr(), new_js_string(ctx, value));
    entry
}

/// Refreshes an `attributes`/`NamedNodeMap` object's indexed `{name, value}`
/// entries and `length` from every live attribute on `id`, same
/// "diff against the last sync, clear trailing indices" convention
/// `sync_class_list` already uses for a shrinking token list.
unsafe fn sync_attributes(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
    object: sys::JSValue,
) {
    let Some(dom::NodeData::Element { attributes, .. }) = (*dom).get(id).map(|node| &node.data)
    else {
        return;
    };
    let mut names: Vec<_> = attributes.keys().cloned().collect();
    names.sort_unstable();
    for (index, name) in names.iter().enumerate() {
        let value = attributes.get(name).cloned().unwrap_or_default();
        let entry = make_attr_entry(ctx, name, &value);
        sys::JS_SetPropertyUint32(ctx, object, index as u32, entry);
    }
    let old_len = ATTRS_LENGTHS
        .with(|reg| {
            reg.borrow()
                .get(&(ctx as usize))
                .and_then(|nodes| nodes.get(&id).copied())
        })
        .unwrap_or(0);
    for index in names.len()..old_len {
        sys::JS_SetPropertyUint32(ctx, object, index as u32, sys::js_undefined());
    }
    let length_name = CString::new("length").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        object,
        length_name.as_ptr(),
        sys::js_float64(names.len() as f64),
    );
    ATTRS_LENGTHS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, names.len())
    });
}

unsafe extern "C" fn attrs_get_named_item(
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
    let Some(id) = attrs_owner(ctx, this_val) else {
        return sys::js_null();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    (*dom)
        .attribute(id, &name)
        .map(|value| make_attr_entry(ctx, &name, value))
        .unwrap_or_else(sys::js_null)
}

unsafe extern "C" fn node_attributes_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    if let Some(value) = ATTRS_OBJECTS.with(|reg| {
        reg.borrow()
            .get(&(ctx as usize))
            .and_then(|objects| objects.get(&id).copied())
    }) {
        let dom = dom_opaque(ctx);
        if !dom.is_null() {
            sync_attributes(ctx, dom, id, value);
        }
        return sys::JS_DupValue(ctx, value);
    }
    let object = sys::JS_NewObject(ctx);
    let proto = array_prototype(ctx);
    sys::JS_SetPrototype(ctx, object, proto);
    sys::JS_FreeValue(ctx, proto);
    let owner = CString::new("__nimbleAttributesOwner").unwrap();
    sys::JS_SetPropertyStr(ctx, object, owner.as_ptr(), sys::JS_DupValue(ctx, this_val));
    let method_name = CString::new("getNamedItem").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        object,
        method_name.as_ptr(),
        sys::JS_NewCFunction2(
            ctx,
            attrs_get_named_item,
            method_name.as_ptr(),
            1,
            sys::JS_CFUNC_GENERIC,
            0,
        ),
    );
    let dom = dom_opaque(ctx);
    if !dom.is_null() {
        sync_attributes(ctx, dom, id, object);
    }
    ATTRS_OBJECTS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, sys::JS_DupValue(ctx, object))
    });
    object
}

unsafe fn define_attributes_collection(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("attributes").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(node_attributes_get),
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
    name.map(|name| new_js_string(ctx, &name))
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

/// Real `Node.prototype.nodeValue` getter — per spec:
/// - Text / Comment nodes: the text data
/// - Document / DocumentFragment / Element: `null`
unsafe extern "C" fn node_node_value_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if node_ptr.is_null() || dom_ptr.is_null() {
        return sys::js_undefined();
    }
    match (*dom_ptr).node_value(*node_ptr) {
        Some(text) => new_js_string(ctx, &text),
        None => sys::js_null(),
    }
}

/// Real `Node.prototype.nodeValue` setter — updates text for Text/Comment
/// nodes, no-op for everything else (Element, Document, DocumentFragment).
unsafe extern "C" fn node_node_value_set(
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
        (*dom_ptr).set_node_value(*node_ptr, &text);
    }
    sys::js_undefined()
}

/// Defines the `nodeValue` accessor on `proto` — per spec, returns the
/// text data for Text/Comment nodes, `null` for everything else.
unsafe fn define_node_value(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("nodeValue").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(node_node_value_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let setter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(node_node_value_set),
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

/// Replaces every child of `id` with the parsed content of `html`, evicting
/// cached JS state (`NODE_OBJECTS`/`CLASS_LIST_OBJECTS`/etc, and any
/// listeners living on those cached objects) for every removed node first —
/// same convention `node_remove`/`node_remove_child` already use, since a
/// removed subtree must not leave stale entries an attacker-controlled
/// `getElementById` could later resurrect a listener on.
unsafe fn replace_children_with_html(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
    html: &str,
) {
    let old_children = (*dom)
        .get(id)
        .map(|node| node.children.clone())
        .unwrap_or_default();
    for child in old_children {
        evict_subtree(ctx, &*dom, child);
        (*dom).remove(child);
    }
    let (fragment, roots) = html::parse_fragment(html);
    for root in roots {
        let cloned = (*dom).adopt(&fragment, root);
        (*dom).append_child(id, cloned);
    }
}

unsafe extern "C" fn node_inner_html_get(
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
    new_js_string(ctx, &(*dom).serialize_children(id))
}

unsafe extern "C" fn node_inner_html_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    // Trusted Types gate (`trusted_types::sink_html_string`): a policy-
    // produced `TrustedHTML` is always accepted; under a delivered
    // `require-trusted-types-for 'script'` CSP directive every plain
    // string throws instead of parsing.
    let html = match crate::trusted_types::sink_html_string(ctx, val, "innerHTML") {
        Ok(html) => html,
        Err(thrown) => return thrown,
    };
    if html.len() > MAX_HTML_LENGTH {
        return throw_type_error(ctx, "innerHTML value exceeds the maximum length");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "innerHTML target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    replace_children_with_html(ctx, dom, id, &html);
    sys::js_undefined()
}

unsafe extern "C" fn node_outer_html_get(
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
    new_js_string(ctx, &(*dom).serialize_node(id))
}

/// Replaces `id` itself, in place under its current parent, with the parsed
/// content of `html` — real `outerHTML` semantics (the node stops existing
/// afterward, matching `node_remove`'s own contract that a real DOM removal
/// makes `this_val` a detached, cache-evicted wrapper from then on).
unsafe extern "C" fn node_outer_html_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    // Same Trusted Types gate as `node_inner_html_set` — `outerHTML` is
    // the other real HTML injection sink this engine exposes.
    let html = match crate::trusted_types::sink_html_string(ctx, val, "outerHTML") {
        Ok(html) => html,
        Err(thrown) => return thrown,
    };
    if html.len() > MAX_HTML_LENGTH {
        return throw_type_error(ctx, "outerHTML value exceeds the maximum length");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "outerHTML target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if id == (*dom).root() {
        return throw_type_error(ctx, "document root cannot be replaced");
    }
    if (*dom).get(id).and_then(|node| node.parent).is_none() {
        return throw_type_error(ctx, "node has no parent to replace it under");
    }
    let (fragment, roots) = html::parse_fragment(&html);
    for root in roots {
        let cloned = (*dom).adopt(&fragment, root);
        (*dom).insert_before(id, cloned);
    }
    evict_subtree(ctx, &*dom, id);
    (*dom).remove(id);
    sys::js_undefined()
}

/// Real `Element.insertAdjacentHTML(position, html)`: parses `html` as a
/// fragment (same `html::parse_fragment` + `Dom::adopt` pipeline
/// `innerHTML`/`outerHTML` already use) and inserts the resulting roots at
/// one of the four real positions relative to this node — `beforebegin`/
/// `afterend` need a parent to insert into and throw `NoModificationAllowedError`-
/// style (a plain `TypeError`, same convention every thrown condition in
/// this file uses) when this node has none, matching the real spec.
/// Goes through the same `trusted_types::sink_html_string` gate as
/// `innerHTML`/`outerHTML` — this engine's three real HTML injection sinks
/// share identical Trusted Types enforcement.
unsafe extern "C" fn node_insert_adjacent_html(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "position and html are required");
    }
    let Some(position) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "position must be a string");
    };
    if !matches!(
        position.as_str(),
        "beforebegin" | "afterbegin" | "beforeend" | "afterend"
    ) {
        return throw_type_error(
            ctx,
            "position must be one of beforebegin/afterbegin/beforeend/afterend",
        );
    }
    let html = match crate::trusted_types::sink_html_string(ctx, *argv.add(1), "insertAdjacentHTML")
    {
        Ok(html) => html,
        Err(thrown) => return thrown,
    };
    if html.len() > MAX_HTML_LENGTH {
        return throw_type_error(ctx, "insertAdjacentHTML value exceeds the maximum length");
    }
    let Some(this_id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "insertAdjacentHTML target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(this_id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    let parent = (*dom).get(this_id).and_then(|node| node.parent);
    if matches!(position.as_str(), "beforebegin" | "afterend") && parent.is_none() {
        return throw_type_error(ctx, "no parent to insert relative to this node");
    }
    let (fragment, roots) = html::parse_fragment(&html);
    match position.as_str() {
        "beforebegin" => {
            for root in roots {
                let cloned = (*dom).adopt(&fragment, root);
                (*dom).insert_before(this_id, cloned);
            }
        }
        "afterbegin" => {
            let first_child = first_child_of(&*dom, this_id);
            for root in roots {
                let cloned = (*dom).adopt(&fragment, root);
                match first_child {
                    Some(sibling) => (*dom).insert_before(sibling, cloned),
                    None => (*dom).append_child(this_id, cloned),
                }
            }
        }
        "beforeend" => {
            for root in roots {
                let cloned = (*dom).adopt(&fragment, root);
                (*dom).append_child(this_id, cloned);
            }
        }
        "afterend" => {
            let reference = next_sibling_of(&*dom, this_id);
            for root in roots {
                let cloned = (*dom).adopt(&fragment, root);
                match reference {
                    Some(sibling) => (*dom).insert_before(sibling, cloned),
                    None => (*dom).append_child(parent.unwrap(), cloned),
                }
            }
        }
        _ => unreachable!(),
    }
    sys::js_undefined()
}

unsafe fn define_inner_outer_html(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, getter, setter) in [
        (
            "innerHTML",
            node_inner_html_get as Getter,
            node_inner_html_set as Setter,
        ),
        (
            "outerHTML",
            node_outer_html_get as Getter,
            node_outer_html_set as Setter,
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

unsafe extern "C" fn node_has_attribute(
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
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    sys::js_bool(!dom.is_null() && (*dom).attribute(id, &name).is_some())
}

unsafe extern "C" fn node_get_attribute_names(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let array = sys::JS_NewArray(ctx);
    let Some(id) = node_id(ctx, this_val) else {
        return array;
    };
    let dom = dom_opaque(ctx);
    let Some(dom::NodeData::Element { attributes, .. }) = (!dom.is_null())
        .then(|| (*dom).get(id).map(|node| &node.data))
        .flatten()
    else {
        return array;
    };
    let mut names: Vec<_> = attributes.keys().collect();
    names.sort_unstable();
    for (index, name) in names.into_iter().enumerate() {
        sys::JS_SetPropertyUint32(ctx, array, index as u32, new_js_string(ctx, name));
    }
    array
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

/// Real `Node.prototype.contains(other)`: `true` when `other` is this node
/// itself or a descendant of it. A non-`Node` (or missing) argument, or a
/// `this` that isn't attached to a live document, is `false` rather than a
/// thrown error — same permissive style `matches`/`closest` already use for
/// a malformed `this`/argument.
unsafe extern "C" fn node_contains(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    if argc < 1 {
        return sys::js_bool(false);
    }
    let Some(other) = node_id(ctx, *argv) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    sys::js_bool(!dom.is_null() && (*dom).contains(id, other))
}

unsafe extern "C" fn node_compare_document_position(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_float64(1.0);
    };
    if argc < 1 {
        return sys::js_float64(1.0);
    }
    let Some(other) = node_id(ctx, *argv) else {
        return sys::js_float64(1.0);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_float64(1.0);
    }
    sys::js_float64((*dom).compare_document_position(id, other) as f64)
}

unsafe extern "C" fn node_normalize(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if !dom.is_null() {
        (*dom).normalize(id);
    }
    sys::js_undefined()
}

/// Real `Node.prototype.cloneNode(deep)`. Returns a brand-new `Node` object
/// via [`node_object`], establishing its own object identity in the cache —
/// same as `document.createElement`/`createTextNode` do for a freshly
/// created id, not the identity of the node it was cloned from.
unsafe extern "C" fn node_clone_node(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "cloneNode target must be a node");
    };
    let deep = argc > 0 && sys::JS_ToBool(ctx, *argv) != 0;
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    let new_id = (*dom).clone_node(id, deep);
    let class_id = node_class_id_for(ctx, dom, new_id);
    node_object(ctx, class_id, new_id)
}

/// Real `Node.prototype.replaceChild(newChild, oldChild)`. Evicts
/// `oldChild`'s cached JS state (identity, listeners, `classList`/`dataset`/
/// `attributes` objects) first — same convention `removeChild`/`remove`
/// already use before a subtree stops existing — but only once membership
/// is confirmed, so a rejected call (old child not actually a child) never
/// evicts a node that's still fully attached elsewhere. Rejects `newChild`
/// being this node's own ancestor, same `HierarchyRequestError`-style check
/// `appendChild`/`insertBefore` already make.
unsafe extern "C" fn node_replace_child(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "new child and old child are required");
    }
    let Some(parent) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "replace target must be a node");
    };
    let new_value = *argv;
    let old_value = *argv.add(1);
    let (Some(new_child), Some(old_child)) = (node_id(ctx, new_value), node_id(ctx, old_value))
    else {
        return throw_type_error(ctx, "replaceChild arguments must be nodes");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null()
        || (*dom).get(parent).is_none()
        || (*dom).get(new_child).is_none()
        || (*dom).get(old_child).is_none()
    {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if is_ancestor(&*dom, new_child, parent) {
        return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
    }
    let old_child_is_member = (*dom)
        .get(parent)
        .map(|node| node.children.contains(&old_child))
        .unwrap_or(false);
    if !old_child_is_member {
        return throw_type_error(ctx, "old child is not a child of this parent");
    }
    if old_child != new_child {
        evict_subtree(ctx, &*dom, old_child);
    }
    (*dom).replace_child(parent, new_child, old_child);
    sys::JS_DupValue(ctx, old_value)
}

fn next_sibling_of(dom: &dom::Dom, id: dom::NodeId) -> Option<dom::NodeId> {
    let parent = dom.get(id)?.parent?;
    let siblings = &dom.get(parent)?.children;
    let index = siblings.iter().position(|&c| c == id)?;
    siblings.get(index + 1).copied()
}

fn first_child_of(dom: &dom::Dom, id: dom::NodeId) -> Option<dom::NodeId> {
    dom.get(id)?.children.first().copied()
}

/// Resolves the variadic argument list `prepend`/`append`/`before`/`after`/
/// `replaceWith` all share: each argument is either a real string primitive
/// (`arg.tag == JS_TAG_STRING`, checked before any coercion — `read_js_string`
/// itself would happily stringify a `Node` object via its own `toString`,
/// which is not what a `Node` argument means here) turned into a fresh Text
/// node, or an existing `Node` object reused by id. Bounded by
/// `MAX_CHILD_NODE_ARGS`/`MAX_TEXT_NODE_LENGTH`, same convention every other
/// untrusted-input entry point in this file already follows. On error,
/// returns the already-thrown exception value for the caller to return
/// directly.
unsafe fn read_child_node_args(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> Result<Vec<dom::NodeId>, sys::JSValue> {
    if argc as usize > MAX_CHILD_NODE_ARGS {
        return Err(throw_type_error(ctx, "too many arguments"));
    }
    let mut nodes = Vec::with_capacity(argc.max(0) as usize);
    for i in 0..argc {
        let arg = *argv.add(i as usize);
        if arg.tag == sys::JS_TAG_STRING {
            let Some(text) = read_js_string(ctx, arg) else {
                return Err(throw_type_error(ctx, "invalid string argument"));
            };
            if text.len() > MAX_TEXT_NODE_LENGTH {
                return Err(throw_type_error(
                    ctx,
                    "text argument exceeds the maximum length",
                ));
            }
            nodes.push((*dom).create_text(&text));
        } else if let Some(id) = node_id(ctx, arg) {
            if (*dom).get(id).is_none() {
                return Err(throw_type_error(
                    ctx,
                    "node is no longer attached to this document",
                ));
            }
            nodes.push(id);
        } else {
            return Err(throw_type_error(
                ctx,
                "arguments must be a Node or a string",
            ));
        }
    }
    Ok(nodes)
}

/// Real `ParentNode.prepend(...nodes)`: inserts each argument, in order, as
/// this node's new leading children. A no-op on a `this` that isn't a live
/// node (real spec permissiveness — same degrade-gracefully convention
/// every other accessor here already follows for a malformed `this`).
unsafe extern "C" fn node_prepend(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(this_id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(this_id).is_none() {
        return sys::js_undefined();
    }
    let nodes = match read_child_node_args(ctx, dom, argc, argv) {
        Ok(nodes) => nodes,
        Err(exception) => return exception,
    };
    for &node in &nodes {
        if is_ancestor(&*dom, node, this_id) {
            return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
        }
    }
    let first_child = first_child_of(&*dom, this_id);
    for node in nodes {
        match first_child {
            Some(sibling) => (*dom).insert_before(sibling, node),
            None => (*dom).append_child(this_id, node),
        }
    }
    sys::js_undefined()
}

/// Real `ParentNode.append(...nodes)`: inserts each argument, in order, as
/// this node's new trailing children.
unsafe extern "C" fn node_append(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(this_id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(this_id).is_none() {
        return sys::js_undefined();
    }
    let nodes = match read_child_node_args(ctx, dom, argc, argv) {
        Ok(nodes) => nodes,
        Err(exception) => return exception,
    };
    for &node in &nodes {
        if is_ancestor(&*dom, node, this_id) {
            return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
        }
    }
    for node in nodes {
        (*dom).append_child(this_id, node);
    }
    sys::js_undefined()
}

/// Real `ChildNode.before(...nodes)`: inserts each argument, in order,
/// immediately before this node in its parent. A no-op if this node has no
/// parent, matching the real spec's own "if parent is null, then return".
unsafe extern "C" fn node_before(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(this_id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    let Some(parent) = (*dom).get(this_id).and_then(|node| node.parent) else {
        return sys::js_undefined();
    };
    let nodes = match read_child_node_args(ctx, dom, argc, argv) {
        Ok(nodes) => nodes,
        Err(exception) => return exception,
    };
    for &node in &nodes {
        if is_ancestor(&*dom, node, parent) {
            return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
        }
    }
    for node in nodes {
        (*dom).insert_before(this_id, node);
    }
    sys::js_undefined()
}

/// Real `ChildNode.after(...nodes)`: inserts each argument, in order,
/// immediately after this node in its parent. The anchor (this node's
/// original next sibling, if any) is captured once before any insertion —
/// every argument lands ahead of it, in argument order.
unsafe extern "C" fn node_after(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(this_id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    let Some(parent) = (*dom).get(this_id).and_then(|node| node.parent) else {
        return sys::js_undefined();
    };
    let nodes = match read_child_node_args(ctx, dom, argc, argv) {
        Ok(nodes) => nodes,
        Err(exception) => return exception,
    };
    for &node in &nodes {
        if is_ancestor(&*dom, node, parent) {
            return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
        }
    }
    let reference = next_sibling_of(&*dom, this_id);
    for node in nodes {
        match reference {
            Some(sibling) => (*dom).insert_before(sibling, node),
            None => (*dom).append_child(parent, node),
        }
    }
    sys::js_undefined()
}

/// Real `ChildNode.replaceWith(...nodes)`: removes this node from its
/// parent and inserts each argument, in order, at the position it
/// occupied. A no-op if this node has no parent. Frees this node's own
/// subtree exactly like `removeChild`/`remove` already do — same
/// documented simplification (a real DOM keeps a removed node alive and
/// reattachable; this one destroys it).
unsafe extern "C" fn node_replace_with(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(this_id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    let Some(parent) = (*dom).get(this_id).and_then(|node| node.parent) else {
        return sys::js_undefined();
    };
    let nodes = match read_child_node_args(ctx, dom, argc, argv) {
        Ok(nodes) => nodes,
        Err(exception) => return exception,
    };
    for &node in &nodes {
        if is_ancestor(&*dom, node, parent) {
            return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
        }
    }
    let reference = next_sibling_of(&*dom, this_id);
    evict_subtree(ctx, &*dom, this_id);
    (*dom).remove(this_id);
    for node in nodes {
        if (*dom).get(node).is_none() {
            // Only reachable if `this` itself was passed as one of the
            // replacement nodes — already freed by the removal above, same
            // graceful "stale id silently drops" degradation every other
            // permissive accessor in this file already has.
            continue;
        }
        match reference {
            Some(sibling) => (*dom).insert_before(sibling, node),
            None => (*dom).append_child(parent, node),
        }
    }
    sys::js_undefined()
}

unsafe fn define_mutation_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, function, arity) in [
        ("appendChild", node_append_child as sys::JSCFunction, 1),
        ("getAttribute", node_get_attribute as sys::JSCFunction, 1),
        ("hasAttribute", node_has_attribute as sys::JSCFunction, 1),
        (
            "getAttributeNames",
            node_get_attribute_names as sys::JSCFunction,
            0,
        ),
        ("setAttribute", node_set_attribute as sys::JSCFunction, 2),
        (
            "removeAttribute",
            node_remove_attribute as sys::JSCFunction,
            1,
        ),
        ("insertBefore", node_insert_before as sys::JSCFunction, 2),
        ("removeChild", node_remove_child as sys::JSCFunction, 1),
        ("remove", node_remove as sys::JSCFunction, 0),
        ("contains", node_contains as sys::JSCFunction, 1),
        ("cloneNode", node_clone_node as sys::JSCFunction, 0),
        ("replaceChild", node_replace_child as sys::JSCFunction, 2),
        ("prepend", node_prepend as sys::JSCFunction, 0),
        ("append", node_append as sys::JSCFunction, 0),
        ("before", node_before as sys::JSCFunction, 0),
        ("after", node_after as sys::JSCFunction, 0),
        ("replaceWith", node_replace_with as sys::JSCFunction, 0),
        (
            "insertAdjacentHTML",
            node_insert_adjacent_html as sys::JSCFunction,
            2,
        ),
        (
            "compareDocumentPosition",
            node_compare_document_position as sys::JSCFunction,
            1,
        ),
        ("normalize", node_normalize as sys::JSCFunction, 0),
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

unsafe fn define_selector_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
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
    define_class_list(ctx, proto);
    define_dataset(ctx, proto);
    define_attributes_collection(ctx, proto);
    crate::css_style::define_style(ctx, proto);
    crate::layout_measurement::define_layout_measurement(ctx, proto);
    define_navigation(ctx, proto);
    define_value(ctx, proto);
    define_node_value(ctx, proto);
    define_validation_and_labels(ctx, proto);
    define_inner_outer_html(ctx, proto);
    define_focus_methods(ctx, proto);
    define_mutation_methods(ctx, proto);
    define_selector_methods(ctx, proto);
    crate::events::define_event_target(ctx, proto);
    sys::JS_SetClassProto(ctx, class_id, proto);

    class_id
}

/// All element descendants of `start` whose tag is in `tags`, in document
/// order (iterative DFS, same traversal limits the selector engine uses).
fn descendants_matching_tags(
    dom: &dom::Dom,
    start: dom::NodeId,
    tags: &[&str],
) -> Vec<dom::NodeId> {
    let mut result = Vec::new();
    let mut stack = vec![start];
    let mut visits = 0usize;
    while let Some(id) = stack.pop() {
        visits += 1;
        if visits > MAX_SELECTOR_VISITS || result.len() > MAX_SELECTOR_RESULTS {
            break;
        }
        if let Some(node) = dom.get(id) {
            if id != start {
                if let dom::NodeData::Element { tag, .. } = &node.data {
                    if tags.contains(&tag.as_str()) {
                        result.push(id);
                    }
                }
            }
            stack.extend(node.children.iter().rev().copied());
        }
    }
    result
}

/// Defines `HTMLFormElement`-specific properties: `elements` (an
/// HTMLCollection of this form's controls), plus no-op `submit()`/`reset()`
/// methods (no network layer to submit to — they exist so real-world form
/// code doesn't throw).
unsafe fn define_form_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let cname = CString::new("elements").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(form_elements_get),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);

    for name in ["submit", "reset"] {
        let cname = CString::new(name).unwrap();
        let value = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<sys::JSCFunction, sys::JSCFunction>(element_scroll_noop),
            cname.as_ptr(),
            0,
            sys::JS_CFUNC_GENERIC,
            0,
        );
        sys::JS_SetPropertyStr(ctx, proto, cname.as_ptr(), value);
    }
}

unsafe extern "C" fn form_elements_get(
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
    let controls = descendants_matching_tags(&*dom, id, &["input", "select", "textarea", "button"]);
    html_collection(ctx, controls)
}

/// Defines `HTMLSelectElement`-specific properties: `options`
/// (HTMLCollection of its `<option>`s), `selectedIndex`, and `value`.
///
/// Documented deviation from browsers for a fresh single-line `<select>`:
/// nothing is implicitly auto-selected here, so an untouched select reads
/// `selectedIndex === -1` / `value === ""` until a script (or markup)
/// selects one explicitly.
unsafe fn define_select_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let cname = CString::new("options").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(select_options_get),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);

    for (name, getter_fn, setter_fn) in [
        (
            "selectedIndex",
            select_selected_index_get as Getter,
            select_selected_index_set as Setter,
        ),
        (
            "value",
            select_value_get as Getter,
            select_value_set as Setter,
        ),
    ] {
        let cname = CString::new(name).unwrap();
        let getter = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<Getter, sys::JSCFunction>(getter_fn),
            cname.as_ptr(),
            0,
            sys::JS_CFUNC_GETTER,
            0,
        );
        let setter = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<Setter, sys::JSCFunction>(setter_fn),
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
}

unsafe extern "C" fn select_options_get(
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
    html_collection(ctx, descendants_matching_tags(&*dom, id, &["option"]))
}

/// An option's effective per-spec value: its `value` attribute when present,
/// otherwise its text content.
fn option_effective_value(dom: &dom::Dom, option_id: dom::NodeId) -> String {
    match dom.attribute(option_id, "value") {
        Some(v) => v.to_string(),
        None => dom.text_content(option_id),
    }
}

unsafe extern "C" fn select_selected_index_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::JSValue {
            u: sys::JSValueUnion { int32: -1 },
            tag: sys::JS_TAG_INT,
        };
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::JSValue {
            u: sys::JSValueUnion { int32: -1 },
            tag: sys::JS_TAG_INT,
        };
    }
    for (index, option) in descendants_matching_tags(&*dom, id, &["option"])
        .into_iter()
        .enumerate()
    {
        if (*dom).attribute(option, "selected").is_some() {
            return sys::JSValue {
                u: sys::JSValueUnion {
                    int32: index as i32,
                },
                tag: sys::JS_TAG_INT,
            };
        }
    }
    sys::JSValue {
        u: sys::JSValueUnion { int32: -1 },
        tag: sys::JS_TAG_INT,
    }
}

unsafe extern "C" fn select_selected_index_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    let index = if val.tag == sys::JS_TAG_INT {
        val.u.int32
    } else {
        -1
    };
    let options = descendants_matching_tags(&*dom, id, &["option"]);
    for option in &options {
        (*dom).remove_attribute(*option, "selected");
    }
    if index >= 0 {
        if let Some(option) = options.get(index as usize) {
            (*dom).set_attribute(*option, "selected", "");
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn select_value_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return new_js_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return new_js_string(ctx, "");
    }
    for option in descendants_matching_tags(&*dom, id, &["option"]) {
        if (*dom).attribute(option, "selected").is_some() {
            return new_js_string(ctx, &option_effective_value(&*dom, option));
        }
    }
    new_js_string(ctx, "")
}

unsafe extern "C" fn select_value_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    let Some(wanted) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if dom.is_null() {
        return sys::js_undefined();
    }
    let options = descendants_matching_tags(&*dom, id, &["option"]);
    for option in &options {
        (*dom).remove_attribute(*option, "selected");
    }
    for option in &options {
        if option_effective_value(&*dom, *option) == wanted {
            (*dom).set_attribute(*option, "selected", "");
            break;
        }
    }
    sys::js_undefined()
}

/// `DOMParser` global — a documented deviation from the spec: instead of a
/// separate `Document`, `parseFromString(html, type)` parses into a fresh
/// detached `DocumentFragment` in the shared `Dom`, so the parsed roots can
/// be queried (`fragment.querySelector(...)`) and later adopted/attached.
unsafe extern "C" fn dom_parser_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    let name = CString::new("parseFromString").unwrap();
    let function = sys::JS_NewCFunction2(
        ctx,
        dom_parser_parse_from_string,
        name.as_ptr(),
        2,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), function);
    obj
}

unsafe extern "C" fn dom_parser_parse_from_string(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(html) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "parseFromString expects an HTML string");
    };
    if html.len() > MAX_HTML_LENGTH {
        return throw_type_error(ctx, "HTML exceeds the maximum length");
    }
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "no document to parse into");
    }
    let (fragment, roots) = html::parse_fragment(&html);
    let container = (*dom).create_document_fragment();
    for root in roots {
        let cloned = (*dom).adopt(&fragment, root);
        (*dom).append_child(container, cloned);
    }
    let class_id = node_class_id_for(ctx, dom, container);
    node_object(ctx, class_id, container)
}

/// `XMLSerializer` global — `serializeToString(node)` returns
/// `dom::Dom::serialize_node`'s HTML serialization of any node.
unsafe extern "C" fn xml_serializer_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    let name = CString::new("serializeToString").unwrap();
    let function = sys::JS_NewCFunction2(
        ctx,
        xml_serializer_serialize_to_string,
        name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), function);
    obj
}

unsafe extern "C" fn xml_serializer_serialize_to_string(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "serializeToString expects a node");
    }
    let Some(id) = node_id(ctx, *argv) else {
        return throw_type_error(ctx, "serializeToString target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    new_js_string(ctx, &(*dom).serialize_node(id))
}

/// Own-property key `setCustomValidity` stashes the message under — kept
/// on the JS wrapper object itself rather than in Rust-side state, so it
/// follows object identity for free and dies with it.
const CUSTOM_VALIDITY_KEY: &str = "__nimbleCustomValidity";

/// Per-spec validity flags this engine can actually compute from DOM
/// state: `valueMissing` (`required` + empty effective value), `tooShort`
/// /`tooLong` (`minlength`/`maxlength` vs the effective value, only when
/// non-empty), and `customError` (a message set via `setCustomValidity`).
/// The remaining spec flags (`typeMismatch`, `patternMismatch`,
/// `rangeUnderflow`/`rangeOverflow`, `stepMismatch`) are always `false` —
/// no email/url/pattern/range validation exists (no regex engine on the
/// native side), a documented cut.
fn validity_flags(
    dom: &dom::Dom,
    id: dom::NodeId,
    custom_error: bool,
) -> Vec<(&'static str, bool)> {
    let value = dom.value(id);
    let required = dom.attribute(id, "required").is_some();
    let char_count = value.chars().count() as i64;
    let parse_len = |attr: &str| -> Option<i64> {
        dom.attribute(id, attr)
            .and_then(|raw| raw.trim().parse::<i64>().ok())
            .filter(|n| *n >= 0)
    };
    let too_long = match parse_len("maxlength") {
        Some(max) => !value.is_empty() && char_count > max,
        None => false,
    };
    let too_short = match parse_len("minlength") {
        Some(min) => !value.is_empty() && char_count < min,
        None => false,
    };
    let flags = vec![
        ("valueMissing", required && value.is_empty()),
        ("typeMismatch", false),
        ("patternMismatch", false),
        ("rangeUnderflow", false),
        ("rangeOverflow", false),
        ("stepMismatch", false),
        ("tooLong", too_long),
        ("tooShort", too_short),
        ("customError", custom_error),
    ];
    flags
}

/// Whether all nine failure flags are false (`validity.valid`'s value).
fn flags_are_valid(dom: &dom::Dom, id: dom::NodeId, custom_error: bool) -> bool {
    validity_flags(dom, id, custom_error)
        .into_iter()
        .all(|(_, bad)| !bad)
}

/// Reads this wrapper's stashed custom-validity message ("" when none).
/// Tag-checked before conversion: `JS_ToCStringLen2` would happily coerce
/// a missing property's `undefined` into the literal string "undefined".
unsafe fn read_custom_validity(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> String {
    if this_val.tag != sys::JS_TAG_OBJECT {
        return String::new();
    }
    let key = CString::new(CUSTOM_VALIDITY_KEY).unwrap();
    let stored = sys::JS_GetPropertyStr(ctx, this_val, key.as_ptr());
    let message = if stored.tag == sys::JS_TAG_STRING {
        read_js_string(ctx, stored).unwrap_or_default()
    } else {
        String::new()
    };
    sys::JS_FreeValue(ctx, stored);
    message
}

/// Real `HTMLElement.validity` — a fresh plain object per access carrying
/// the ten per-spec boolean flags (see `validity_flags` for which are
/// computed vs always-false).
unsafe extern "C" fn validity_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    let Some(id) = node_id(ctx, this_val) else {
        return obj;
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return obj;
    }
    let custom_message = read_custom_validity(ctx, this_val);
    let flags = validity_flags(&*dom, id, !custom_message.is_empty());
    for (name, bad) in &flags {
        let cname = CString::new(*name).unwrap();
        sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_bool(*bad));
    }
    let valid = flags.iter().all(|(_, bad)| !bad);
    let cname = CString::new("valid").unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_bool(valid));
    obj
}

/// Real `setCustomValidity(message)` — stores the message on the wrapper
/// (empty string clears it), feeding `customError`/`validationMessage`.
unsafe extern "C" fn validity_set_custom(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "setCustomValidity expects a message");
    }
    let message = read_js_string(ctx, *argv).unwrap_or_default();
    if this_val.tag != sys::JS_TAG_OBJECT {
        return sys::js_undefined();
    }
    let key = CString::new(CUSTOM_VALIDITY_KEY).unwrap();
    sys::JS_SetPropertyStr(ctx, this_val, key.as_ptr(), new_js_string(ctx, &message));
    sys::js_undefined()
}

/// Real `validationMessage` — the custom message when one is set (making
/// `customError` true), otherwise "" (no built-in constraint produces
/// messages here).
unsafe extern "C" fn validation_message_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    new_js_string(ctx, &read_custom_validity(ctx, this_val))
}

/// Real `checkValidity()` — computes the flags; on failure fires a real
/// `"invalid"` event on the element first (per-spec order), then returns
/// the result. Always returns true off a non-element or DOM-less context.
unsafe extern "C" fn check_validity(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(true);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_bool(true);
    }
    let custom_message = read_custom_validity(ctx, this_val);
    let valid = flags_are_valid(&*dom, id, !custom_message.is_empty());
    if !valid {
        crate::events::dispatch(ctx, this_val, "invalid");
    }
    sys::js_bool(valid)
}

/// Real `reportValiditity()` stub — same boolean as `checkValidity` but
/// never fires `invalid` and shows nothing (no UI layer). Documented
/// simplification.
unsafe extern "C" fn report_validity(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(true);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_bool(true);
    }
    let custom_message = read_custom_validity(ctx, this_val);
    let valid = flags_are_valid(&*dom, id, !custom_message.is_empty());
    sys::js_bool(valid)
}

/// `willValidate` — true for named-form-control tags that aren't disabled;
/// every other refinement of the real spec's eligibility rules (readonly,
/// hidden/reset/button/file types, datalist descendants) is a documented
/// cut.
unsafe extern "C" fn will_validate_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_bool(false);
    }
    let eligible = match (*dom).get(id).map(|node| &node.data) {
        Some(dom::NodeData::Element { tag, .. }) => {
            matches!(tag.as_str(), "input" | "select" | "textarea")
        }
        _ => false,
    };
    sys::js_bool(eligible && (*dom).attribute(id, "disabled").is_none())
}

/// Real `labels` for labelable elements — `<label for="id">` matches
/// anywhere in the document plus every `<label>` *ancestor* (implicit
/// wrapping). Documented deviation: not re-sorted into full tree order
/// across the two sources (for= matches come first).
unsafe extern "C" fn labels_get(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::JS_NewArray(ctx);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::JS_NewArray(ctx);
    }
    let mut result: Vec<dom::NodeId> = Vec::new();
    let element_id = (*dom).attribute(id, "id").unwrap_or("").to_string();
    if !element_id.is_empty() {
        if let Ok(labels) = matching_by_tag(&*dom, (*dom).root(), "label", false) {
            for label in labels {
                if (*dom).attribute(label, "for").map(|f| f == element_id) == Some(true) {
                    result.push(label);
                }
            }
        }
    }
    // Wrapping ancestors (nearest first — good enough for the documented order).
    let mut current = (*dom).get(id).and_then(|node| node.parent);
    while let Some(ancestor) = current {
        let parent = (*dom).get(ancestor).and_then(|node| node.parent);
        if let Some(dom::NodeData::Element { tag, .. }) = (*dom).get(ancestor).map(|n| &n.data) {
            if tag == "label" {
                result.push(ancestor);
            }
        }
        current = parent;
    }
    html_collection(ctx, result)
}

/// Defines the constraint-validation surface + `labels` on
/// `Node.prototype` (this engine's single generic class carries every
/// Element-only API there).
unsafe fn define_validation_and_labels(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let cname = CString::new("validity").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(validity_get),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);

    let cname = CString::new("validationMessage").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(validation_message_get),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);

    let cname = CString::new("willValidate").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(will_validate_get),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);

    let cname = CString::new("labels").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(labels_get),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);

    for (name, func, length) in [
        ("checkValidity", check_validity as sys::JSCFunction, 0),
        ("reportValidity", report_validity as sys::JSCFunction, 0),
        (
            "setCustomValidity",
            validity_set_custom as sys::JSCFunction,
            1,
        ),
    ] {
        let cname = CString::new(name).unwrap();
        let f = sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, proto, cname.as_ptr(), f);
    }
}

/// Defines scroll-related properties and methods on `Element.prototype`.
/// This engine has no real viewport, so these are stubs: `scrollTop`/`scrollLeft`
/// always return 0, setters are no-ops, and `scroll()`/`scrollTo()`/`scrollBy()`/
/// `scrollIntoView()` are no-ops. Exists so real-world code that calls these
/// methods doesn't throw.
unsafe fn define_scroll_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
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

unsafe extern "C" fn element_scroll_noop(
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

/// Registers the `Element` class with prototype chained to `Node.prototype`.
unsafe fn ensure_element_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let existing = crate::class_registry::class_id_for(rt, ELEMENT_CLASS_KIND);
    if existing != 0 {
        return existing;
    }
    let class_name = CString::new("Element").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(node_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, ELEMENT_CLASS_KIND, &def);

    let node_proto = sys::JS_GetClassProto(ctx, ensure_node_class(ctx));
    let proto = sys::JS_NewObject(ctx);
    sys::JS_SetPrototype(ctx, proto, node_proto);
    sys::JS_FreeValue(ctx, node_proto);
    define_scroll_methods(ctx, proto);
    sys::JS_SetClassProto(ctx, class_id, proto);

    class_id
}

/// Registers the `HTMLElement` class with prototype chained to `Element.prototype`.
unsafe fn ensure_html_element_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let existing = crate::class_registry::class_id_for(rt, HTML_ELEMENT_CLASS_KIND);
    if existing != 0 {
        return existing;
    }
    let class_name = CString::new("HTMLElement").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(node_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, HTML_ELEMENT_CLASS_KIND, &def);

    let element_proto = sys::JS_GetClassProto(ctx, ensure_element_class(ctx));
    let proto = sys::JS_NewObject(ctx);
    sys::JS_SetPrototype(ctx, proto, element_proto);
    sys::JS_FreeValue(ctx, element_proto);
    sys::JS_SetClassProto(ctx, class_id, proto);

    class_id
}

/// Registers a concrete HTML element subclass (e.g. HTMLInputElement) with
/// prototype chained to `HTMLElement.prototype`.
unsafe fn ensure_html_subclass(ctx: *mut sys::JSContext, kind: &'static str) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let existing = crate::class_registry::class_id_for(rt, kind);
    if existing != 0 {
        return existing;
    }
    let class_name = CString::new(kind).unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(node_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, kind, &def);

    let html_element_proto = sys::JS_GetClassProto(ctx, ensure_html_element_class(ctx));
    let proto = sys::JS_NewObject(ctx);
    sys::JS_SetPrototype(ctx, proto, html_element_proto);
    sys::JS_FreeValue(ctx, html_element_proto);
    if kind == HTML_FORM_CLASS_KIND {
        define_form_properties(ctx, proto);
    } else if kind == HTML_SELECT_CLASS_KIND {
        define_select_properties(ctx, proto);
    }
    sys::JS_SetClassProto(ctx, class_id, proto);

    class_id
}

/// Exposes a global `Name` constructor whose `.prototype` is `proto`.
unsafe fn expose_constructor(
    ctx: *mut sys::JSContext,
    name: &str,
    constructor_fn: sys::JSCFunction,
    proto: sys::JSValue,
) {
    let cname = CString::new(name).unwrap();
    let constructor = sys::JS_NewCFunction2(
        ctx,
        constructor_fn,
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetConstructorBit(ctx, constructor, true);
    let proto_for_prop = sys::JS_DupValue(ctx, proto);
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(ctx, constructor, proto_name.as_ptr(), proto_for_prop);
    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, cname.as_ptr(), constructor);
    sys::JS_FreeValue(ctx, global);
}

/// Minimal no-op constructor for the interface hierarchy.
/// These exist solely to provide `instanceof` support — the real node
/// creation always goes through `make_node_object` with the correct class_id.
unsafe extern "C" fn node_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn element_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_element_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_input_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_button_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_anchor_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_image_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_canvas_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_form_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_select_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
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
        Some(node_id) => {
            let class_id = node_class_id_for(ctx, dom_ptr, node_id);
            node_object(ctx, class_id, node_id)
        }
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
    let class_id = node_class_id_for(ctx, dom, id);
    node_object(ctx, class_id, id)
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
    let class_id = node_class_id_for(ctx, dom, id);
    node_object(ctx, class_id, id)
}

unsafe extern "C" fn document_create_comment(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "comment data is required");
    }
    let Some(text) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "comment data must be a string");
    };
    if text.len() > MAX_TEXT_NODE_LENGTH {
        return throw_type_error(ctx, "comment exceeds the maximum length");
    }
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "document is unavailable");
    }
    let id = (*dom).create_comment(&text);
    let class_id = node_class_id_for(ctx, dom, id);
    node_object(ctx, class_id, id)
}

unsafe extern "C" fn document_create_document_fragment(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "document is unavailable");
    }
    let id = (*dom).create_document_fragment();
    let class_id = node_class_id_for(ctx, dom, id);
    node_object(ctx, class_id, id)
}

/// Real `document.importNode(node, deep?)`: clones a node into this
/// document. Since this engine has a single browsing context (one `Dom`
/// arena), this is equivalent to `cloneNode(deep)` — the `deep` parameter
/// defaults to `true` per the modern spec.
unsafe extern "C" fn document_import_node(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "node is required");
    }
    let Some(source_id) = node_id(ctx, *argv) else {
        return throw_type_error(ctx, "argument must be a Node");
    };
    let deep = if argc >= 2 {
        let b = sys::JS_ToBool(ctx, *argv.add(1));
        b >= 0 && b != 0
    } else {
        true
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "document is unavailable");
    }
    let cloned = (*dom).clone_node(source_id, deep);
    let class_id = node_class_id_for(ctx, dom, cloned);
    node_object(ctx, class_id, cloned)
}

/// Real `document.adoptNode(node)`: moves a node into this document.
/// Since this engine has a single browsing context (one `Dom` arena),
/// the node is already in this document — returns it as-is.
unsafe extern "C" fn document_adopt_node(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "node is required");
    }
    let Some(id) = node_id(ctx, *argv) else {
        return throw_type_error(ctx, "argument must be a Node");
    };
    let class_id = node_class_id_for(ctx, dom_opaque(ctx), id);
    node_object(ctx, class_id, id)
}

unsafe extern "C" fn document_get_elements_by_tag_name(
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
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::JS_NewArray(ctx);
    }
    elements_by_tag_name(ctx, (*dom_ptr).root(), &tag, true)
}

unsafe extern "C" fn document_get_elements_by_class_name(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "class name is required");
    }
    let Some(class_name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "class name must be a string");
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::JS_NewArray(ctx);
    }
    elements_by_class_name(ctx, (*dom_ptr).root(), &class_name, true)
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
        Some(node_id) => {
            let class_id = node_class_id_for(ctx, dom_ptr, node_id);
            node_object(ctx, class_id, node_id)
        }
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
                let class_id = node_class_id_for(ctx, dom, id);
                node_object(ctx, class_id, id)
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
                let class_id = node_class_id_for(ctx, dom, id);
                node_object(ctx, class_id, id)
            })
            .unwrap_or_else(sys::js_null),
        Err(_) => sys::js_null(),
    }
}

unsafe extern "C" fn document_head_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    match matching_nodes(&*dom, (*dom).root(), "head", true) {
        Ok(nodes) => nodes
            .into_iter()
            .next()
            .map(|id| {
                let class_id = node_class_id_for(ctx, dom, id);
                node_object(ctx, class_id, id)
            })
            .unwrap_or_else(sys::js_null),
        Err(_) => sys::js_null(),
    }
}

/// Real `document.URL` — the full serialization of the page's current URL,
/// or `""` if no URL is set.
unsafe extern "C" fn document_url_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    match crate::location::current_url(ctx) {
        Some(url) => new_js_string(ctx, &url.to_string()),
        None => new_js_string(ctx, ""),
    }
}

/// Real `document.baseURI` — same as `document.URL` in this single-context
/// engine (no `<base>` element parsing).
unsafe extern "C" fn document_base_uri_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    match crate::location::current_url(ctx) {
        Some(url) => new_js_string(ctx, &url.to_string()),
        None => new_js_string(ctx, ""),
    }
}

/// Returns a live snapshot JS array of all matching elements under the
/// document root — shared helper for `forms`/`images`/`links`/`scripts`.
unsafe fn document_elements_by_tag(ctx: *mut sys::JSContext, tag: &str) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::JS_NewArray(ctx);
    }
    match matching_by_tag(&*dom, (*dom).root(), tag, false) {
        Ok(nodes) => html_collection(ctx, nodes),
        Err(_) => sys::JS_NewArray(ctx),
    }
}

/// Real `document.forms` — all `<form>` elements in the document.
unsafe extern "C" fn document_forms_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    document_elements_by_tag(ctx, "form")
}

/// Real `document.images` — all `<img>` elements in the document.
unsafe extern "C" fn document_images_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    document_elements_by_tag(ctx, "img")
}

/// Real `document.links` — all `<a>` and `<area>` elements with an `href`
/// attribute in the document.
unsafe extern "C" fn document_links_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::JS_NewArray(ctx);
    }
    let mut result = Vec::new();
    if let Ok(a_nodes) = matching_by_tag(&*dom, (*dom).root(), "a", false) {
        for id in a_nodes {
            if (*dom).attribute(id, "href").is_some() {
                result.push(id);
            }
        }
    }
    if let Ok(area_nodes) = matching_by_tag(&*dom, (*dom).root(), "area", false) {
        for id in area_nodes {
            if (*dom).attribute(id, "href").is_some() {
                result.push(id);
            }
        }
    }
    html_collection(ctx, result)
}

/// Real `document.scripts` — all `<script>` elements in the document.
unsafe extern "C" fn document_scripts_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    document_elements_by_tag(ctx, "script")
}

/// Real `document.readyState`, hardcoded to `"complete"`. This engine has
/// no `loading`/`interactive` distinction to report — `DOMContentLoaded`/
/// `load` already fire at the right real moments (`Context::
/// dispatch_lifecycle_events`), but nothing tracks a mid-parse state a
/// script reading this *during* its own execution could observe as
/// anything other than "the document I'm running in is done". Same
/// documented-placeholder convention `page_visibility`'s always-`"visible"`
/// state already uses for an unmodeled real API.
unsafe extern "C" fn document_ready_state_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    new_js_string(ctx, "complete")
}

/// Real `document.title` read side: the text content of the first
/// `<title>` element in document order, or `""` if none exists — matches
/// the real DOM's own fallback.
unsafe extern "C" fn document_title_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return new_js_string(ctx, "");
    }
    match matching_nodes(&*dom, (*dom).root(), "title", true) {
        Ok(nodes) => match nodes.into_iter().next() {
            Some(id) => new_js_string(ctx, &(*dom).text_content(id)),
            None => new_js_string(ctx, ""),
        },
        Err(_) => new_js_string(ctx, ""),
    }
}

/// Real `document.title` write side: updates the first existing `<title>`
/// element's text if one exists. Otherwise creates one and appends it to
/// `<head>` if present, else `<html>` (`documentElement`), else the
/// document root itself — a narrower fallback chain than the real spec's
/// (which also handles SVG documents specially), but this engine only ever
/// parses/builds HTML documents.
unsafe extern "C" fn document_title_set(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    let Some(text) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if dom.is_null() {
        return sys::js_undefined();
    }
    let existing = matching_nodes(&*dom, (*dom).root(), "title", true)
        .ok()
        .and_then(|nodes| nodes.into_iter().next());
    if let Some(id) = existing {
        (*dom).set_text_content(id, &text);
        return sys::js_undefined();
    }
    let title_id = (*dom).create_element("title");
    (*dom).set_text_content(title_id, &text);
    let parent = matching_nodes(&*dom, (*dom).root(), "head", true)
        .ok()
        .and_then(|nodes| nodes.into_iter().next())
        .or_else(|| {
            matching_nodes(&*dom, (*dom).root(), "html", true)
                .ok()
                .and_then(|nodes| nodes.into_iter().next())
        })
        .unwrap_or_else(|| (*dom).root());
    (*dom).append_child(parent, title_id);
    sys::js_undefined()
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

unsafe fn define_head(ctx: *mut sys::JSContext, document: sys::JSValue) {
    let name = CString::new("head").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(document_head_get),
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

unsafe fn define_ready_state(ctx: *mut sys::JSContext, document: sys::JSValue) {
    let name = CString::new("readyState").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(document_ready_state_get),
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

unsafe fn define_title(ctx: *mut sys::JSContext, document: sys::JSValue) {
    let name = CString::new("title").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(document_title_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let setter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(document_title_set),
        name.as_ptr(),
        1,
        sys::JS_CFUNC_SETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        document,
        atom,
        getter,
        setter,
        sys::JS_PROP_HAS_GET | sys::JS_PROP_HAS_SET | sys::JS_PROP_CONFIGURABLE,
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
    ensure_element_class(ctx);
    ensure_html_element_class(ctx);
    ensure_html_subclass(ctx, HTML_INPUT_CLASS_KIND);
    ensure_html_subclass(ctx, HTML_BUTTON_CLASS_KIND);
    ensure_html_subclass(ctx, HTML_ANCHOR_CLASS_KIND);
    ensure_html_subclass(ctx, HTML_IMAGE_CLASS_KIND);
    ensure_html_subclass(ctx, HTML_CANVAS_CLASS_KIND);
    ensure_html_subclass(ctx, HTML_FORM_CLASS_KIND);
    ensure_html_subclass(ctx, HTML_SELECT_CLASS_KIND);

    let rt = sys::JS_GetRuntime(ctx);
    let node_class_id = crate::class_registry::class_id_for(rt, NODE_CLASS_KIND);
    let element_class_id = crate::class_registry::class_id_for(rt, ELEMENT_CLASS_KIND);
    let html_element_class_id = crate::class_registry::class_id_for(rt, HTML_ELEMENT_CLASS_KIND);

    let node_proto = sys::JS_GetClassProto(ctx, node_class_id);
    expose_constructor(ctx, "Node", node_constructor, node_proto);
    sys::JS_FreeValue(ctx, node_proto);

    let element_proto = sys::JS_GetClassProto(ctx, element_class_id);
    expose_constructor(ctx, "Element", element_constructor, element_proto);
    sys::JS_FreeValue(ctx, element_proto);

    let html_element_proto = sys::JS_GetClassProto(ctx, html_element_class_id);
    expose_constructor(
        ctx,
        "HTMLElement",
        html_element_constructor,
        html_element_proto,
    );
    sys::JS_FreeValue(ctx, html_element_proto);

    for (kind, ctor_fn) in [
        (
            HTML_INPUT_CLASS_KIND,
            html_input_constructor as sys::JSCFunction,
        ),
        (
            HTML_BUTTON_CLASS_KIND,
            html_button_constructor as sys::JSCFunction,
        ),
        (
            HTML_ANCHOR_CLASS_KIND,
            html_anchor_constructor as sys::JSCFunction,
        ),
        (
            HTML_IMAGE_CLASS_KIND,
            html_image_constructor as sys::JSCFunction,
        ),
        (
            HTML_CANVAS_CLASS_KIND,
            html_canvas_constructor as sys::JSCFunction,
        ),
        (
            HTML_FORM_CLASS_KIND,
            html_form_constructor as sys::JSCFunction,
        ),
        (
            HTML_SELECT_CLASS_KIND,
            html_select_constructor as sys::JSCFunction,
        ),
    ] {
        let class_id = crate::class_registry::class_id_for(rt, kind);
        let proto = sys::JS_GetClassProto(ctx, class_id);
        expose_constructor(ctx, kind, ctor_fn, proto);
        sys::JS_FreeValue(ctx, proto);
    }

    for (name, ctor_fn) in [
        ("DOMParser", dom_parser_constructor as sys::JSCFunction),
        (
            "XMLSerializer",
            xml_serializer_constructor as sys::JSCFunction,
        ),
    ] {
        let empty_proto = sys::JS_NewObject(ctx);
        expose_constructor(ctx, name, ctor_fn, empty_proto);
        sys::JS_FreeValue(ctx, empty_proto);
    }

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
        ("createComment", document_create_comment as sys::JSCFunction),
        (
            "createDocumentFragment",
            document_create_document_fragment as sys::JSCFunction,
        ),
        (
            "getElementsByTagName",
            document_get_elements_by_tag_name as sys::JSCFunction,
        ),
        (
            "getElementsByClassName",
            document_get_elements_by_class_name as sys::JSCFunction,
        ),
        ("importNode", document_import_node as sys::JSCFunction),
        ("adoptNode", document_adopt_node as sys::JSCFunction),
    ] {
        let name = CString::new(name).unwrap();
        let value =
            sys::JS_NewCFunction2(ctx, function, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, document, name.as_ptr(), value);
    }
    define_active_element(ctx, document);
    define_body(ctx, document);
    define_head(ctx, document);
    define_title(ctx, document);
    define_ready_state(ctx, document);
    define_document_element(ctx, document);
    for (name, getter) in [
        ("URL", document_url_get as Getter),
        ("baseURI", document_base_uri_get as Getter),
        ("forms", document_forms_get as Getter),
        ("images", document_images_get as Getter),
        ("links", document_links_get as Getter),
        ("scripts", document_scripts_get as Getter),
    ] {
        let cname = CString::new(name).unwrap();
        let getter_fn = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<Getter, sys::JSCFunction>(getter),
            cname.as_ptr(),
            0,
            sys::JS_CFUNC_GETTER,
            0,
        );
        let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
        sys::JS_DefinePropertyGetSet(
            ctx,
            document,
            atom,
            getter_fn,
            sys::js_undefined(),
            sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
        );
        sys::JS_FreeAtom(ctx, atom);
    }

    sys::JS_FreeValue(ctx, document);
}
