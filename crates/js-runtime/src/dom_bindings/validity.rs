//! Constraint-validation surface (`validity`/`checkValidity`/
//! `willValidate`/`setCustomValidity`) and `labels`, split out from
//! `forms.rs`.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::collections::html_collection;
use super::node_registry::{dom_opaque, node_id};
use super::selectors::matching_by_tag;
use super::util::{new_js_string, read_js_string, throw_type_error, Getter};

/// Own-property key `setCustomValidity` stashes the message under — kept
/// on the JS wrapper object itself rather than in Rust-side state, so it
/// follows object identity for free and dies with it.
const CUSTOM_VALIDITY_KEY: &str = "__atomicCustomValidity";

/// Per-spec validity flags this engine can actually compute from DOM
/// state: `valueMissing` (`required` + empty effective value), `tooShort`
/// /`tooLong` (`minlength`/`maxlength` vs the effective value, only when
/// non-empty), and `customError` (a message set via `setCustomValidity`).
/// The remaining spec flags (`typeMismatch`, `patternMismatch`,
/// `rangeUnderflow`/`rangeOverflow`, `stepMismatch`) are always `false` —
/// no email/url/pattern/range validation exists (no regex engine on the
/// native side), a documented cut.
fn validity_flags(
    dom: &dom::Dom,
    id: dom::NodeId,
    custom_error: bool,
) -> Vec<(&'static str, bool)> {
    let value = dom.value(id);
    let required = dom.attribute(id, "required").is_some();
    let char_count = value.chars().count() as i64;
    let parse_len = |attr: &str| -> Option<i64> {
        dom.attribute(id, attr)
            .and_then(|raw| raw.trim().parse::<i64>().ok())
            .filter(|n| *n >= 0)
    };
    let too_long = match parse_len("maxlength") {
        Some(max) => !value.is_empty() && char_count > max,
        None => false,
    };
    let too_short = match parse_len("minlength") {
        Some(min) => !value.is_empty() && char_count < min,
        None => false,
    };
    let flags = vec![
        ("valueMissing", required && value.is_empty()),
        ("typeMismatch", false),
        ("patternMismatch", false),
        ("rangeUnderflow", false),
        ("rangeOverflow", false),
        ("stepMismatch", false),
        ("tooLong", too_long),
        ("tooShort", too_short),
        ("customError", custom_error),
    ];
    flags
}

/// Whether all nine failure flags are false (`validity.valid`'s value).
fn flags_are_valid(dom: &dom::Dom, id: dom::NodeId, custom_error: bool) -> bool {
    validity_flags(dom, id, custom_error)
        .into_iter()
        .all(|(_, bad)| !bad)
}

/// Reads this wrapper's stashed custom-validity message ("" when none).
/// Tag-checked before conversion: `JS_ToCStringLen2` would happily coerce
/// a missing property's `undefined` into the literal string "undefined".
unsafe fn read_custom_validity(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> String {
    if this_val.tag != sys::JS_TAG_OBJECT {
        return String::new();
    }
    let key = CString::new(CUSTOM_VALIDITY_KEY).unwrap();
    let stored = sys::JS_GetPropertyStr(ctx, this_val, key.as_ptr());
    let message = if stored.tag == sys::JS_TAG_STRING {
        read_js_string(ctx, stored).unwrap_or_default()
    } else {
        String::new()
    };
    sys::JS_FreeValue(ctx, stored);
    message
}

/// Real `HTMLElement.validity` — a fresh plain object per access carrying
/// the ten per-spec boolean flags (see `validity_flags` for which are
/// computed vs always-false).
unsafe extern "C" fn validity_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    let Some(id) = node_id(ctx, this_val) else {
        return obj;
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return obj;
    }
    let custom_message = read_custom_validity(ctx, this_val);
    let flags = validity_flags(&*dom, id, !custom_message.is_empty());
    for (name, bad) in &flags {
        let cname = CString::new(*name).unwrap();
        sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_bool(*bad));
    }
    let valid = flags.iter().all(|(_, bad)| !bad);
    let cname = CString::new("valid").unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_bool(valid));
    obj
}

/// Real `setCustomValidity(message)` — stores the message on the wrapper
/// (empty string clears it), feeding `customError`/`validationMessage`.
unsafe extern "C" fn validity_set_custom(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "setCustomValidity expects a message");
    }
    let message = read_js_string(ctx, *argv).unwrap_or_default();
    if this_val.tag != sys::JS_TAG_OBJECT {
        return sys::js_undefined();
    }
    let key = CString::new(CUSTOM_VALIDITY_KEY).unwrap();
    sys::JS_SetPropertyStr(ctx, this_val, key.as_ptr(), new_js_string(ctx, &message));
    sys::js_undefined()
}

/// Real `validationMessage` — the custom message when one is set (making
/// `customError` true), otherwise "" (no built-in constraint produces
/// messages here).
unsafe extern "C" fn validation_message_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    new_js_string(ctx, &read_custom_validity(ctx, this_val))
}

/// Real `checkValidity()` — computes the flags; on failure fires a real
/// `"invalid"` event on the element first (per-spec order), then returns
/// the result. Always returns true off a non-element or DOM-less context.
pub(super) unsafe extern "C" fn check_validity(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(true);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_bool(true);
    }
    let custom_message = read_custom_validity(ctx, this_val);
    let valid = flags_are_valid(&*dom, id, !custom_message.is_empty());
    if !valid {
        crate::events::dispatch(ctx, this_val, "invalid");
    }
    sys::js_bool(valid)
}

/// Real `reportValiditity()` stub — same boolean as `checkValidity` but
/// never fires `invalid` and shows nothing (no UI layer). Documented
/// simplification.
unsafe extern "C" fn report_validity(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(true);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_bool(true);
    }
    let custom_message = read_custom_validity(ctx, this_val);
    let valid = flags_are_valid(&*dom, id, !custom_message.is_empty());
    sys::js_bool(valid)
}

/// `willValidate` — true for named-form-control tags that aren't disabled;
/// every other refinement of the real spec's eligibility rules (readonly,
/// hidden/reset/button/file types, datalist descendants) is a documented
/// cut.
unsafe extern "C" fn will_validate_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_bool(false);
    }
    let eligible = match (*dom).get(id).map(|node| &node.data) {
        Some(dom::NodeData::Element { tag, .. }) => {
            matches!(tag.as_str(), "input" | "select" | "textarea")
        }
        _ => false,
    };
    sys::js_bool(eligible && (*dom).attribute(id, "disabled").is_none())
}

/// Real `labels` for labelable elements — `<label for="id">` matches
/// anywhere in the document plus every `<label>` *ancestor* (implicit
/// wrapping). Documented deviation: not re-sorted into full tree order
/// across the two sources (for= matches come first).
unsafe extern "C" fn labels_get(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::JS_NewArray(ctx);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::JS_NewArray(ctx);
    }
    let mut result: Vec<dom::NodeId> = Vec::new();
    let element_id = (*dom).attribute(id, "id").unwrap_or("").to_string();
    if !element_id.is_empty() {
        if let Ok(labels) = matching_by_tag(&*dom, (*dom).root(), "label", false) {
            for label in labels {
                if (*dom).attribute(label, "for").map(|f| f == element_id) == Some(true) {
                    result.push(label);
                }
            }
        }
    }
    // Wrapping ancestors (nearest first — good enough for the documented order).
    let mut current = (*dom).get(id).and_then(|node| node.parent);
    while let Some(ancestor) = current {
        let parent = (*dom).get(ancestor).and_then(|node| node.parent);
        if let Some(dom::NodeData::Element { tag, .. }) = (*dom).get(ancestor).map(|n| &n.data) {
            if tag == "label" {
                result.push(ancestor);
            }
        }
        current = parent;
    }
    html_collection(ctx, result)
}

/// Defines the constraint-validation surface + `labels` on
/// `Node.prototype` (this engine's single generic class carries every
/// Element-only API there).
pub(super) unsafe fn define_validation_and_labels(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let cname = CString::new("validity").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(validity_get),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);

    let cname = CString::new("validationMessage").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(validation_message_get),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);

    let cname = CString::new("willValidate").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(will_validate_get),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);

    let cname = CString::new("labels").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(labels_get),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);

    for (name, func, length) in [
        ("checkValidity", check_validity as sys::JSCFunction, 0),
        ("reportValidity", report_validity as sys::JSCFunction, 0),
        (
            "setCustomValidity",
            validity_set_custom as sys::JSCFunction,
            1,
        ),
    ] {
        let cname = CString::new(name).unwrap();
        let f = sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, proto, cname.as_ptr(), f);
    }
}
