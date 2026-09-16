//! Wires each `document` property getter/setter from `document_properties.rs`
//! onto the live `document` object — split out from `document.rs`.

use std::ffi::CString;

use quickjs_sys as sys;

use super::document_properties::{
    document_active_element_get, document_body_get, document_document_element_get,
    document_head_get, document_ready_state_get, document_title_get, document_title_set,
};
use super::util::{Getter, Setter};

pub(super) unsafe fn define_active_element(ctx: *mut sys::JSContext, document: sys::JSValue) {
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

pub(super) unsafe fn define_body(ctx: *mut sys::JSContext, document: sys::JSValue) {
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

pub(super) unsafe fn define_head(ctx: *mut sys::JSContext, document: sys::JSValue) {
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

pub(super) unsafe fn define_ready_state(ctx: *mut sys::JSContext, document: sys::JSValue) {
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

pub(super) unsafe fn define_title(ctx: *mut sys::JSContext, document: sys::JSValue) {
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

pub(super) unsafe fn define_document_element(ctx: *mut sys::JSContext, document: sys::JSValue) {
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
