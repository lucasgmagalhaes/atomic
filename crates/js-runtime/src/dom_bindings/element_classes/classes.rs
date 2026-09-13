//! `ensure_*_class` — quickjs class + prototype registration for the
//! `Node`/`Element`/`HTMLElement`/HTML-subclass hierarchy, split out from
//! `element_classes.rs`.

use std::ffi::CString;

use quickjs_sys as sys;

use crate::dom_bindings::attributes::{define_attribute_properties, define_attributes_collection};
use crate::dom_bindings::class_list::define_class_list;
use crate::dom_bindings::collections::define_selector_methods;
use crate::dom_bindings::content::{
    define_default_checked, define_default_selected, define_default_value, define_inner_outer_html,
    define_node_value, define_selection_properties, define_text_content, define_value,
};
use crate::dom_bindings::dataset::define_dataset;
use crate::dom_bindings::forms::define_form_properties;
use crate::dom_bindings::mutation::define_mutation_methods;
use crate::dom_bindings::navigation::define_navigation;
use crate::dom_bindings::node_registry::{
    node_opaque, ELEMENT_CLASS_KIND, HTML_ELEMENT_CLASS_KIND, HTML_FORM_CLASS_KIND,
    HTML_SELECT_CLASS_KIND, NODE_CLASS_KIND,
};
use crate::dom_bindings::scroll_focus::{define_focus_methods, define_scroll_methods};
use crate::dom_bindings::select::define_select_properties;
use crate::dom_bindings::validity::define_validation_and_labels;

unsafe extern "C" fn node_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = node_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

/// Registers the `Node` class on `ctx`'s runtime (if not already done for
/// this runtime) and builds this context's `Node.prototype`.
pub(super) unsafe fn ensure_node_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
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
    define_default_checked(ctx, proto);
    define_default_selected(ctx, proto);
    define_selection_properties(ctx, proto);
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
pub(super) unsafe fn ensure_element_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
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
pub(super) unsafe fn ensure_html_element_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
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
pub(super) unsafe fn ensure_html_subclass(
    ctx: *mut sys::JSContext,
    kind: &'static str,
) -> sys::JSClassID {
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
