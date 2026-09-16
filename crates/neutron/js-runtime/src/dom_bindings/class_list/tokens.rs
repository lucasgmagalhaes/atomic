//! `class_tokens`/`valid_class_token`/`array_prototype` — split out from
//! `class_list.rs`.

use std::ffi::CString;

use quickjs_sys as sys;

/// Also used by `selectors.rs`'s `matching_by_class`, which needs the same
/// whitespace-tokenizing rule for a class-name argument that isn't backed
/// by any live node.
pub(in super::super) fn class_tokens(value: &str) -> Vec<String> {
    value.split_ascii_whitespace().map(str::to_owned).collect()
}

pub(super) fn valid_class_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= crate::dom_bindings::util::MAX_ATTRIBUTE_NAME_LENGTH
        && !token.bytes().any(|byte| byte.is_ascii_whitespace())
}

/// Fetches `Array.prototype` so a classList object can inherit it via
/// `JS_SetPrototype` — the cheapest real way to get a genuine, spec-shaped
/// `Symbol.iterator`/`forEach`/`entries`/... for free on an array-like
/// object, since this crate's `quickjs-sys` bindings don't expose a way to
/// define a well-known-symbol property directly.
///
/// Also used by `attributes.rs`'s `node_attributes_get` for the same
/// `NamedNodeMap`-shaped array-like object.
pub(in super::super) unsafe fn array_prototype(ctx: *mut sys::JSContext) -> sys::JSValue {
    let global = sys::JS_GetGlobalObject(ctx);
    let array_name = CString::new("Array").unwrap();
    let array_ctor = sys::JS_GetPropertyStr(ctx, global, array_name.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let proto_name = CString::new("prototype").unwrap();
    let proto = sys::JS_GetPropertyStr(ctx, array_ctor, proto_name.as_ptr());
    sys::JS_FreeValue(ctx, array_ctor);
    proto
}
