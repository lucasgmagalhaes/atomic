//! Text/value/HTML content accessors on `Node.prototype`: `textContent`,
//! `value`, `nodeValue`, and the three real HTML injection sinks
//! (`innerHTML`/`outerHTML`/`insertAdjacentHTML`), all gated through
//! `crate::trusted_types` where applicable.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::mutation::{evict_subtree, first_child_of, next_sibling_of};
use super::node_registry::{dom_opaque, node_id, node_opaque};
use super::util::{
    new_js_string, read_js_string, throw_type_error, Getter, Setter, MAX_HTML_LENGTH,
};

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
pub(super) unsafe fn define_text_content(ctx: *mut sys::JSContext, proto: sys::JSValue) {
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
pub(super) unsafe fn define_value(ctx: *mut sys::JSContext, proto: sys::JSValue) {
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
pub(super) unsafe fn define_node_value(ctx: *mut sys::JSContext, proto: sys::JSValue) {
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
/// cached JS state (`node_registry`'s object-identity cache and each
/// submodule's own per-node cache, and any listeners living on those cached
/// objects) for every removed node first — same convention
/// `mutation::node_remove`/`node_remove_child` already use, since a removed
/// subtree must not leave stale entries an attacker-controlled
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
pub(crate) unsafe extern "C" fn node_insert_adjacent_html(
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

pub(super) unsafe fn define_inner_outer_html(ctx: *mut sys::JSContext, proto: sys::JSValue) {
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

/// Only `node_insert_adjacent_html` needs `node_insert_adjacent_html` to be
/// wired up on `Node.prototype` — done by `define_mutation_methods` in
/// `mutation.rs` instead of here, since it's grouped with the other
/// `Node.prototype` *methods* (as opposed to accessor pairs) there. Exposed
/// with `pub(super)` for that wiring to reach it.
pub(crate) use node_insert_adjacent_html as insert_adjacent_html;
