//! The `FormData` native constructor — resolves an explicit prototype
//! (`new_target`'s own, else `globalThis.FormData.prototype`, same
//! resolution order `blob.rs` uses) and optionally populates from a passed
//! `<form>` element via [`super::form_populate::populate_from_form`].

use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

use super::form_populate::populate_from_form;
use super::types::{Entry, FormDataInner};
use super::util::FORM_DATA_CLASS_KIND;

pub(super) unsafe extern "C" fn form_data_constructor(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let mut entries: Vec<Entry> = Vec::new();
    if argc >= 1 && (*argv).tag == sys::JS_TAG_OBJECT {
        populate_from_form(ctx, *argv, &mut entries);
    }

    // Same explicit-prototype resolution `blob.rs` uses: `new_target`'s
    // `.prototype` wins, else `globalThis.FormData.prototype` — the
    // per-context implicit default has proven unreliable under concurrent
    // Runtime construction (see `resolve_prototype` there).
    let class_id =
        crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), FORM_DATA_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    let mut proto = if new_target.tag != sys::JS_TAG_UNDEFINED {
        let proto_name = CString::new("prototype").unwrap();
        sys::JS_GetPropertyStr(ctx, new_target, proto_name.as_ptr())
    } else {
        sys::js_undefined()
    };
    if proto.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, proto);
        let global = sys::JS_GetGlobalObject(ctx);
        let ctor_name = CString::new("FormData").unwrap();
        let ctor = sys::JS_GetPropertyStr(ctx, global, ctor_name.as_ptr());
        sys::JS_FreeValue(ctx, global);
        let proto_name = CString::new("prototype").unwrap();
        proto = sys::JS_GetPropertyStr(ctx, ctor, proto_name.as_ptr());
        sys::JS_FreeValue(ctx, ctor);
    }
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);
    sys::JS_SetOpaque(
        obj,
        Box::into_raw(Box::new(FormDataInner { entries })) as *mut c_void,
    );
    obj
}
