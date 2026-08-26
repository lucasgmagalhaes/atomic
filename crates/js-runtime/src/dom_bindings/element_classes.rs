//! Registers the `Node`/`Element`/`HTMLElement`/HTML-subclass interface
//! hierarchy: one real `quickjs-sys` class per level (`ensure_node_class`
//! and friends), chained prototypes, and a minimal no-op constructor per
//! level exposed globally purely so `instanceof` works — real node creation
//! always goes through [`make_node_object`] with the correct `class_id`.

use std::ffi::CString;
use std::os::raw::c_void;

use quickjs_sys as sys;

use super::attributes::{define_attribute_properties, define_attributes_collection};
use super::class_list::define_class_list;
use super::collections::define_selector_methods;
use super::content::{
    define_default_value, define_inner_outer_html, define_node_value, define_text_content,
    define_value,
};
use super::dataset::define_dataset;
use super::forms::{
    define_form_properties, define_select_properties, define_validation_and_labels,
};
use super::mutation::define_mutation_methods;
use super::navigation::define_navigation;
use super::node_registry::{
    node_opaque, ELEMENT_CLASS_KIND, HTML_ANCHOR_CLASS_KIND, HTML_BUTTON_CLASS_KIND,
    HTML_CANVAS_CLASS_KIND, HTML_ELEMENT_CLASS_KIND, HTML_FORM_CLASS_KIND, HTML_IMAGE_CLASS_KIND,
    HTML_INPUT_CLASS_KIND, HTML_SELECT_CLASS_KIND, NODE_CLASS_KIND,
};
use super::scroll_focus::{define_focus_methods, define_scroll_methods};

unsafe extern "C" fn node_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = node_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
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
    let existing = sys::JS_GetClassProto(ctx, class_id);
    let already_registered = existing.tag == sys::JS_TAG_OBJECT;
    sys::JS_FreeValue(ctx, existing);
    if already_registered {
        return class_id;
    }

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
    define_default_value(ctx, proto);
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

/// Registers the `Element` class with prototype chained to `Node.prototype`.
unsafe fn ensure_element_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("Element").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(node_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, ELEMENT_CLASS_KIND, &def);
    let existing = sys::JS_GetClassProto(ctx, class_id);
    let already_registered = existing.tag == sys::JS_TAG_OBJECT;
    sys::JS_FreeValue(ctx, existing);
    if already_registered {
        return class_id;
    }

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
    let class_name = CString::new("HTMLElement").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(node_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, HTML_ELEMENT_CLASS_KIND, &def);
    let existing = sys::JS_GetClassProto(ctx, class_id);
    let already_registered = existing.tag == sys::JS_TAG_OBJECT;
    sys::JS_FreeValue(ctx, existing);
    if already_registered {
        return class_id;
    }

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
    let class_name = CString::new(kind).unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(node_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, kind, &def);
    let existing = sys::JS_GetClassProto(ctx, class_id);
    let already_registered = existing.tag == sys::JS_TAG_OBJECT;
    sys::JS_FreeValue(ctx, existing);
    if already_registered {
        return class_id;
    }

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

/// Exposes a global `Name` constructor whose `.prototype` is `proto`. Also
/// used by `document.rs` for the `DOMParser`/`XMLSerializer` globals.
pub(super) unsafe fn expose_constructor(
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
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn element_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_element_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_input_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_button_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_anchor_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_image_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_canvas_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_form_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

unsafe extern "C" fn html_select_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

/// Also used by `node_registry::node_object` to build the actual JS object
/// for a real `dom::NodeId` once identity-cache lookup misses.
pub(super) unsafe fn make_node_object(
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

/// Registers the whole `Node`/`Element`/`HTMLElement`/HTML-subclass
/// interface hierarchy and its no-op `instanceof`-only constructors — the
/// class-registration half of what used to be the flat file's `register`.
/// `document::install` handles the rest (the `document` global itself plus
/// `DOMParser`/`XMLSerializer`).
pub(super) unsafe fn install_classes(ctx: *mut sys::JSContext) {
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
}
