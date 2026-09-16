//! `DOMParser`/`XMLSerializer` globals, plus `document.write`/`writeln` —
//! split out from `document.rs`.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_class_id_for, node_id, node_object};
use super::selectors::matching_nodes;
use super::util::{new_js_string, read_js_string, throw_type_error, MAX_HTML_LENGTH};

/// `DOMParser` global — a documented deviation from the spec: instead of a
/// separate `Document`, `parseFromString(html, type)` parses into a fresh
/// detached `DocumentFragment` in the shared `Dom`, so the parsed roots can
/// be queried (`fragment.querySelector(...)`) and later adopted/attached.
pub(super) unsafe extern "C" fn dom_parser_constructor(
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
pub(super) unsafe extern "C" fn xml_serializer_constructor(
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

/// `document.write`/`document.writeln` — real, but deliberately narrower
/// than spec: this engine has no streaming HTML parser to re-enter (the
/// whole document is fully parsed, by `html5ever`, before any page script
/// ever runs — see `profile-worker`'s `page/load.rs`'s own real
/// script-execution-order doc), so there is no "currently open parser,
/// insert at the tokenizer's own insertion point" case to support. Real
/// spec's *other* documented case — `write()` called after the document
/// has already finished loading — is the one this implements: every
/// string argument is concatenated (`writeln` appends a trailing `\n`,
/// same relationship real spec's own `writeln` has to `write`), parsed as
/// an HTML fragment via the same `html::parse_fragment` pipeline
/// `DOMParser.parseFromString` already uses, and the resulting nodes
/// appended to `document.body` (or the document root, if no `<body>`
/// exists yet). Real spec's implicit `document.open()` — which erases
/// the *entire* existing document and starts a brand new one — is *not*
/// replicated: doing so would tear down live script state (listeners,
/// timers, in-flight fetches) this engine has no mechanism to safely
/// discard mid-page, and no caller in this codebase has ever needed that
/// "self-rewriting page" pattern. A page script calling `document.write`
/// mid-parse (the real spec case this can't support) simply gets the
/// same append-only behavior, rather than throwing or silently doing
/// nothing — the same "documented deviation, not a crash" convention
/// `DOMParser`'s single-shared-Dom note above already sets.
unsafe extern "C" fn document_write_impl(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
    newline: bool,
) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "no document to write into");
    }
    let mut html = String::new();
    for i in 0..argc {
        if let Some(piece) = read_js_string(ctx, *argv.add(i as usize)) {
            html.push_str(&piece);
        }
    }
    if newline {
        html.push('\n');
    }
    if html.len() > MAX_HTML_LENGTH {
        return throw_type_error(ctx, "document.write input exceeds the maximum length");
    }

    let target = match matching_nodes(&*dom, (*dom).root(), "body", true) {
        Ok(nodes) => nodes.into_iter().next().unwrap_or_else(|| (*dom).root()),
        Err(_) => (*dom).root(),
    };
    let (fragment, roots) = html::parse_fragment(&html);
    for root in roots {
        let cloned = (*dom).adopt(&fragment, root);
        (*dom).append_child(target, cloned);
    }
    sys::js_undefined()
}

unsafe extern "C" fn document_write(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    document_write_impl(ctx, argc, argv, false)
}

unsafe extern "C" fn document_writeln(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    document_write_impl(ctx, argc, argv, true)
}

pub(super) unsafe fn install_write(ctx: *mut sys::JSContext, document: sys::JSValue) {
    for (name, function) in [
        ("write", document_write as sys::JSCFunction),
        ("writeln", document_writeln as sys::JSCFunction),
    ] {
        let name = CString::new(name).unwrap();
        let value =
            sys::JS_NewCFunction2(ctx, function, name.as_ptr(), 0, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, document, name.as_ptr(), value);
    }
}
