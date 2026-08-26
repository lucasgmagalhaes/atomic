//! The global `document` object's own methods/accessors
//! (`getElementById`/`createElement`/`querySelector`/`body`/`head`/`title`/
//! ...), plus the `DOMParser`/`XMLSerializer` globals — the two other
//! document-adjacent parsing utilities this engine exposes.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::collections::{
    elements_by_class_name, elements_by_tag_name, html_collection, query_selector_all,
};
use super::element_classes::expose_constructor;
use super::mutation::valid_tag_name;
use super::node_registry::{dom_opaque, node_class_id_for, node_id, node_object};
use super::selectors::matching_by_tag;
use super::selectors::matching_nodes;
use super::util::{
    new_js_string, read_js_string, throw_type_error, Getter, Setter, MAX_HTML_LENGTH,
    MAX_TEXT_NODE_LENGTH,
};

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

/// Registers the global `document` object's methods/accessors plus the
/// `DOMParser`/`XMLSerializer` globals. Callers must have already pointed
/// the context's opaque slot at a live `dom::Dom` via `JS_SetContextOpaque`
/// — bindings read it back on every call and no-op (return null/undefined)
/// if unset. The `Node`/`Element`/`HTMLElement`/HTML-subclass interface
/// hierarchy itself is `element_classes::install_classes`'s job, called
/// first by `dom_bindings::register`.
pub(super) unsafe fn install(ctx: *mut sys::JSContext) {
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
