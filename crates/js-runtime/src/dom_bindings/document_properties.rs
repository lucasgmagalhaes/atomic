//! `document.activeElement`/`body`/`documentElement`/`head`/`URL`/
//! `baseURI`/`forms`/`images`/`links`/`scripts`/`readyState`/`title` —
//! property getters/setters, split out from `document.rs`.

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_class_id_for, node_object};
use super::selectors::matching_nodes;
use super::util::{new_js_string, read_js_string};

pub(super) unsafe extern "C" fn document_active_element_get(
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

pub(super) unsafe extern "C" fn document_body_get(
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

pub(super) unsafe extern "C" fn document_document_element_get(
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

pub(super) unsafe extern "C" fn document_head_get(
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
pub(super) unsafe extern "C" fn document_url_get(
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
pub(super) unsafe extern "C" fn document_base_uri_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    match crate::location::current_url(ctx) {
        Some(url) => new_js_string(ctx, &url.to_string()),
        None => new_js_string(ctx, ""),
    }
}

/// Returns a real *live* `HTMLCollection` (`collections::live`'s real
/// `Proxy`-backed re-query, same one `getElementsByTagName` now uses —
/// see that module's own doc) of every `tag` element under the document
/// root — shared helper for `forms`/`images`/`scripts`. `include_start:
/// true` matches `document.getElementsByTagName`'s own convention
/// (root is the document itself, which a tag query still needs to
/// consider even though a `Document` node can never itself match a real
/// tag name).
unsafe fn document_elements_by_tag(ctx: *mut sys::JSContext, tag: &str) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::JS_NewArray(ctx);
    }
    super::collections::live_elements_by_tag_name(ctx, (*dom).root(), tag, true)
}

/// Real `document.forms` — all `<form>` elements in the document.
pub(super) unsafe extern "C" fn document_forms_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    document_elements_by_tag(ctx, "form")
}

/// Real `document.images` — all `<img>` elements in the document.
pub(super) unsafe extern "C" fn document_images_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    document_elements_by_tag(ctx, "img")
}

/// Real `document.links` — all `<a>` and `<area>` elements with an `href`
/// attribute in the document. Real *live* `HTMLCollection` (see
/// `document_elements_by_tag`'s own doc) via `collections::live`'s
/// `Query::Links`.
pub(super) unsafe extern "C" fn document_links_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::JS_NewArray(ctx);
    }
    super::collections::live_links(ctx, (*dom).root(), true)
}

/// Real `document.scripts` — all `<script>` elements in the document.
pub(super) unsafe extern "C" fn document_scripts_get(
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
pub(super) unsafe extern "C" fn document_ready_state_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    new_js_string(ctx, "complete")
}

/// Real `document.title` read side: the text content of the first
/// `<title>` element in document order, or `""` if none exists — matches
/// the real DOM's own fallback.
pub(super) unsafe extern "C" fn document_title_get(
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
pub(super) unsafe extern "C" fn document_title_set(
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
