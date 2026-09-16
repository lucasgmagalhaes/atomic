//! `HTMLSelectElement`-specific properties: `options` (HTMLCollection of
//! its `<option>`s), `selectedIndex`, and `value` — split out from
//! `forms.rs`.
//!
//! Documented deviation from browsers for a fresh single-line `<select>`:
//! nothing is implicitly auto-selected here, so an untouched select reads
//! `selectedIndex === -1` / `value === ""` until a script (or markup)
//! selects one explicitly.

use std::ffi::CString;

use quickjs_sys as sys;

use super::collections::html_collection;
use super::node_registry::{dom_opaque, node_id};
use super::selectors::descendants_matching_tags;
use super::util::{new_js_string, read_js_string, Getter, Setter};

pub(super) unsafe fn define_select_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let cname = CString::new("options").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(select_options_get),
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

    for (name, getter_fn, setter_fn) in [
        (
            "selectedIndex",
            select_selected_index_get as Getter,
            select_selected_index_set as Setter,
        ),
        (
            "value",
            select_value_get as Getter,
            select_value_set as Setter,
        ),
    ] {
        let cname = CString::new(name).unwrap();
        let getter = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<Getter, sys::JSCFunction>(getter_fn),
            cname.as_ptr(),
            0,
            sys::JS_CFUNC_GETTER,
            0,
        );
        let setter = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<Setter, sys::JSCFunction>(setter_fn),
            cname.as_ptr(),
            1,
            sys::JS_CFUNC_SETTER,
            0,
        );
        let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
        sys::JS_DefinePropertyGetSet(
            ctx,
            proto,
            atom,
            getter,
            setter,
            sys::JS_PROP_HAS_GET
                | sys::JS_PROP_HAS_SET
                | sys::JS_PROP_CONFIGURABLE
                | sys::JS_PROP_ENUMERABLE,
        );
        sys::JS_FreeAtom(ctx, atom);
    }
}

unsafe extern "C" fn select_options_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::JS_NewArray(ctx);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::JS_NewArray(ctx);
    }
    html_collection(ctx, descendants_matching_tags(&*dom, id, &["option"]))
}

/// An option's effective per-spec value: its `value` attribute when present,
/// otherwise its text content.
fn option_effective_value(dom: &dom::Dom, option_id: dom::NodeId) -> String {
    match dom.attribute(option_id, "value") {
        Some(v) => v.to_string(),
        None => dom.text_content(option_id),
    }
}

unsafe extern "C" fn select_selected_index_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::JSValue {
            u: sys::JSValueUnion { int32: -1 },
            tag: sys::JS_TAG_INT,
        };
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::JSValue {
            u: sys::JSValueUnion { int32: -1 },
            tag: sys::JS_TAG_INT,
        };
    }
    for (index, option) in descendants_matching_tags(&*dom, id, &["option"])
        .into_iter()
        .enumerate()
    {
        if (*dom).selected(option) {
            return sys::JSValue {
                u: sys::JSValueUnion {
                    int32: index as i32,
                },
                tag: sys::JS_TAG_INT,
            };
        }
    }
    sys::JSValue {
        u: sys::JSValueUnion { int32: -1 },
        tag: sys::JS_TAG_INT,
    }
}

/// Sets the live `.selected` on exactly one `<option>` descendant of
/// `id` (real single-select exclusivity — clearing every other option
/// first), leaving the `selected` *attribute* (and therefore
/// `.defaultSelected`) on every option untouched. Real spec behavior:
/// picking a different option via `selectedIndex`/`.value` doesn't
/// rewrite markup, only `Element.reset()` restores from it — same
/// live-vs-default split `set_checked`/`defaultChecked` already have.
unsafe extern "C" fn select_selected_index_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    let index = if val.tag == sys::JS_TAG_INT {
        val.u.int32
    } else {
        -1
    };
    let options = descendants_matching_tags(&*dom, id, &["option"]);
    for option in &options {
        (*dom).set_selected(*option, false);
    }
    if index >= 0 {
        if let Some(option) = options.get(index as usize) {
            (*dom).set_selected(*option, true);
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn select_value_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return new_js_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return new_js_string(ctx, "");
    }
    for option in descendants_matching_tags(&*dom, id, &["option"]) {
        if (*dom).selected(option) {
            return new_js_string(ctx, &option_effective_value(&*dom, option));
        }
    }
    new_js_string(ctx, "")
}

unsafe extern "C" fn select_value_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    let Some(wanted) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if dom.is_null() {
        return sys::js_undefined();
    }
    let options = descendants_matching_tags(&*dom, id, &["option"]);
    for option in &options {
        (*dom).set_selected(*option, false);
    }
    for option in &options {
        if option_effective_value(&*dom, *option) == wanted {
            (*dom).set_selected(*option, true);
            break;
        }
    }
    sys::js_undefined()
}
