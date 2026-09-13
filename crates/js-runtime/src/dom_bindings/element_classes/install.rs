//! `install_classes` — wires up the `Node`/`Element`/`HTMLElement`/
//! HTML-subclass hierarchy and its no-op `instanceof`-only constructors,
//! split out from `element_classes.rs`.

use quickjs_sys as sys;

use crate::dom_bindings::node_registry::{
    ELEMENT_CLASS_KIND, HTML_ANCHOR_CLASS_KIND, HTML_BUTTON_CLASS_KIND, HTML_CANVAS_CLASS_KIND,
    HTML_ELEMENT_CLASS_KIND, HTML_FORM_CLASS_KIND, HTML_IFRAME_CLASS_KIND, HTML_IMAGE_CLASS_KIND,
    HTML_INPUT_CLASS_KIND, HTML_SELECT_CLASS_KIND, NODE_CLASS_KIND,
};

use super::classes::{
    ensure_element_class, ensure_html_element_class, ensure_html_subclass, ensure_node_class,
};
use super::constructors::{
    element_constructor, expose_constructor, html_anchor_constructor, html_button_constructor,
    html_canvas_constructor, html_element_constructor, html_form_constructor,
    html_iframe_constructor, html_image_constructor, html_input_constructor,
    html_select_constructor, node_constructor,
};

/// Registers the whole `Node`/`Element`/`HTMLElement`/HTML-subclass
/// interface hierarchy and its no-op `instanceof`-only constructors — the
/// class-registration half of what used to be the flat file's `register`.
/// `document::install` handles the rest (the `document` global itself plus
/// `DOMParser`/`XMLSerializer`).
pub(in crate::dom_bindings) unsafe fn install_classes(ctx: *mut sys::JSContext) {
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
    ensure_html_subclass(ctx, HTML_IFRAME_CLASS_KIND);

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
        (
            HTML_IFRAME_CLASS_KIND,
            html_iframe_constructor as sys::JSCFunction,
        ),
    ] {
        let class_id = crate::class_registry::class_id_for(rt, kind);
        let proto = sys::JS_GetClassProto(ctx, class_id);
        expose_constructor(ctx, kind, ctor_fn, proto);
        sys::JS_FreeValue(ctx, proto);
    }
}
