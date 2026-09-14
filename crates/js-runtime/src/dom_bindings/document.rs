//! The global `document` object's own methods/accessors
//! (`getElementById`/`createElement`/`querySelector`/`body`/`head`/`title`/
//! ...), plus the `DOMParser`/`XMLSerializer` globals — the two other
//! document-adjacent parsing utilities this engine exposes.
//!
//! Split into `document_creation.rs` (node-creation/importing methods),
//! `document_query.rs` (getElementsBy*/querySelector*),
//! `document_properties.rs` (property getters/setters),
//! `document_properties_define.rs` (wiring those onto the live object), and
//! `document_parsers.rs` (`DOMParser`/`XMLSerializer`) — this file keeps
//! only the module doc and the single public entry point, `install`.

use std::ffi::CString;

use quickjs_sys as sys;

use super::document_creation::{
    document_adopt_node, document_create_comment, document_create_document_fragment,
    document_create_element, document_create_text_node, document_get_element_by_id,
    document_import_node,
};
use super::document_parsers::{dom_parser_constructor, install_write, xml_serializer_constructor};
use super::document_properties::{
    document_base_uri_get, document_forms_get, document_images_get, document_links_get,
    document_scripts_get, document_url_get,
};
use super::document_properties_define::{
    define_active_element, define_body, define_document_element, define_head, define_ready_state,
    define_title,
};
use super::document_query::{
    document_get_elements_by_class_name, document_get_elements_by_tag_name,
    document_query_selector, document_query_selector_all,
};
use super::element_classes::expose_constructor;
use super::util::Getter;

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
        (
            "createRange",
            crate::selection::document_create_range as sys::JSCFunction,
        ),
        (
            "getSelection",
            crate::selection::get_selection as sys::JSCFunction,
        ),
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

    install_write(ctx, document);

    sys::JS_FreeValue(ctx, document);
}
