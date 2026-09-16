//! `defaultValue`/`defaultChecked`/`defaultSelected` accessors on
//! `Node.prototype` — split out from `content/mod.rs`.

use quickjs_sys as sys;

use crate::js_helpers::define_getter_setter;

use super::super::node_registry::{dom_opaque, node_id};
use super::super::util::{new_js_string, read_js_string};

unsafe extern "C" fn node_default_value_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return new_js_string(ctx, "");
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return new_js_string(ctx, "");
    }
    new_js_string(ctx, (*dom_ptr).attribute(id, "value").unwrap_or_default())
}

unsafe extern "C" fn node_default_value_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let Some(text) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    let dom_ptr = dom_opaque(ctx);
    if !dom_ptr.is_null() {
        (*dom_ptr).set_attribute(id, "value", &text);
    }
    sys::js_undefined()
}

/// Defines the `defaultValue` accessor on `proto` — real, and genuinely
/// independent of `value` (see [`super::text_value::define_value`]):
/// `value` is the live `dom::Dom::value`/`set_value` field, `defaultValue`
/// reflects the `value` *attribute* directly, exactly matching real
/// `HTMLInputElement`/`HTMLTextAreaElement` semantics (an untouched
/// control's `.value` starts equal to `.defaultValue`, then diverges
/// independently once either is set). `Element.reset()`
/// (`forms.rs`'s `form_reset`) uses this to restore a control's live
/// `.value` to its markup default — the one form-reset case this engine
/// can do for real, since `.checked`/`.selected` (`attributes.rs`) are
/// still simplified as direct attribute reflection with no separate
/// "default" storage to restore from (a pre-existing, documented
/// simplification, not one this pass introduces).
pub(in super::super) unsafe fn define_default_value(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    define_getter_setter(
        ctx,
        proto,
        "defaultValue",
        node_default_value_get,
        node_default_value_set,
    );
}

unsafe extern "C" fn node_default_checked_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom_ptr = dom_opaque(ctx);
    sys::js_bool(!dom_ptr.is_null() && (*dom_ptr).attribute(id, "checked").is_some())
}

unsafe extern "C" fn node_default_checked_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom_ptr = dom_opaque(ctx);
    if !dom_ptr.is_null() {
        if sys::JS_ToBool(ctx, val) != 0 {
            (*dom_ptr).set_attribute(id, "checked", "");
        } else {
            (*dom_ptr).remove_attribute(id, "checked");
        }
    }
    sys::js_undefined()
}

/// Defines the `defaultChecked` accessor on `proto` — reflects the
/// `checked` *content attribute* directly, same relationship
/// [`define_default_value`]'s own doc describes between `defaultValue`
/// and `value`: an untouched checkbox/radio's `.checked` starts equal to
/// `.defaultChecked`, then diverges independently once either is set.
/// `forms.rs`'s `form_reset` uses this to restore `.checked` on reset.
pub(in super::super) unsafe fn define_default_checked(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
) {
    define_getter_setter(
        ctx,
        proto,
        "defaultChecked",
        node_default_checked_get,
        node_default_checked_set,
    );
}

unsafe extern "C" fn node_default_selected_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom_ptr = dom_opaque(ctx);
    sys::js_bool(!dom_ptr.is_null() && (*dom_ptr).attribute(id, "selected").is_some())
}

unsafe extern "C" fn node_default_selected_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom_ptr = dom_opaque(ctx);
    if !dom_ptr.is_null() {
        if sys::JS_ToBool(ctx, val) != 0 {
            (*dom_ptr).set_attribute(id, "selected", "");
        } else {
            (*dom_ptr).remove_attribute(id, "selected");
        }
    }
    sys::js_undefined()
}

/// Defines the `defaultSelected` accessor on `proto` — reflects the
/// `selected` *content attribute* directly, same relationship
/// [`define_default_checked`]'s own doc describes between
/// `defaultChecked` and `.checked`: an untouched `<option>`'s `.selected`
/// starts equal to `.defaultSelected`, then diverges independently once
/// either is set. `forms.rs`'s `form_reset` uses this to restore
/// `.selected` on every `<option>` when its owning `<select>` resets.
pub(in super::super) unsafe fn define_default_selected(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
) {
    define_getter_setter(
        ctx,
        proto,
        "defaultSelected",
        node_default_selected_get,
        node_default_selected_set,
    );
}
