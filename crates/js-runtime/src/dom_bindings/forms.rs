//! `HTMLFormElement`-specific properties (`elements`/`reset()`/
//! `requestSubmit()`/`submit()`) and the real default click action for
//! submit/reset buttons and `<a href>`. Split into `select.rs`
//! (`HTMLSelectElement`-specific properties) and `validity.rs` (constraint
//! validation + `labels`), both exposed generically on this engine's single
//! `Node`/subclass prototypes.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::collections::html_collection;
use super::node_registry::{dom_opaque, node_class_id_for, node_id, node_object};
use super::scroll_focus::element_scroll_noop;
use super::selectors::descendants_matching_tags;
use super::util::Getter;
use super::validity::check_validity;

/// A `<button>`/`<input>`'s real default click action, per spec: `<button>`
/// defaults to `type="submit"` when the attribute is absent.
enum ButtonAction {
    Submit,
    Reset,
}

fn default_button_action(dom: &dom::Dom, id: dom::NodeId) -> Option<ButtonAction> {
    let node = dom.get(id)?;
    let dom::NodeData::Element {
        tag, attributes, ..
    } = &node.data
    else {
        return None;
    };
    let type_attr = attributes.get("type").map(String::as_str);
    match tag.as_str() {
        "button" => match type_attr {
            None | Some("submit") => Some(ButtonAction::Submit),
            Some("reset") => Some(ButtonAction::Reset),
            _ => None,
        },
        "input" => match type_attr {
            Some("submit") => Some(ButtonAction::Submit),
            Some("reset") => Some(ButtonAction::Reset),
            _ => None,
        },
        _ => None,
    }
}

/// Real default action for an unprevented `"click"`: on a real `<a href>`,
/// requests navigation (see `host_state::HostState::pending_navigation`'s
/// own doc); on a submit/reset button, triggers its nearest ancestor
/// `<form>`'s real `requestSubmit()`/`reset()` — previously neither ever
/// ran on a real click, only when a script called
/// `form.requestSubmit()`/`.reset()` itself. Called once per unprevented
/// `"click"` from both
/// `events::dispatch` and `events::dispatch_existing` — the two real
/// tree-walking dispatch paths a click on an actual DOM element goes
/// through (`dispatchEvent("click")` and `dispatchEvent(existingEvent)`
/// respectively; `dispatch_event_object` is the third, non-tree-walking
/// path used only for `window`/`document`-shaped targets, which can never
/// be a button), mirroring how a real browser runs an element's default
/// action once its event's bubble phase finishes uncanceled. Reaches the
/// form's real native `form_reset`/`form_request_submit`
/// functions directly (never `ctx.eval` — see `CLAUDE.md`'s own gotcha
/// on re-entrant `JS_Eval` from inside a native callback) via the same
/// `node_object`/`node_class_id_for` real per-`NodeId` JS-object identity
/// `events::dispatch_at` itself already uses to build a listener's `this`.
/// A silent no-op if `target` isn't a submit/reset button, or has no
/// ancestor `<form>` at all (a real, spec-matching outcome — a submit
/// button outside any form does nothing on click).
pub(crate) unsafe fn run_default_click_action(ctx: *mut sys::JSContext, target: sys::JSValue) {
    let Some(id) = node_id(ctx, target) else {
        return;
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return;
    }

    // Real `<a href>` default click action: requests navigation (see
    // `host_state::HostState::pending_navigation`'s own doc for why this
    // engine can only *request* it, not perform it — no fetch/host-process
    // layer reachable from here) rather than triggering a form. Checked
    // first since an anchor is never also a submit/reset control.
    if let Some(dom::NodeData::Element {
        tag, attributes, ..
    }) = (*dom).get(id).map(|n| &n.data)
    {
        if tag == "a" {
            if let Some(href) = attributes.get("href").filter(|h| !h.is_empty()) {
                let state = crate::host_state::get(ctx);
                if !state.is_null() {
                    (*state).pending_navigation = Some(href.clone());
                }
            }
            return;
        }
    }

    let Some(action) = default_button_action(&*dom, id) else {
        return;
    };

    let mut current = (*dom).get(id).and_then(|n| n.parent);
    let form_id = loop {
        let Some(candidate) = current else {
            return;
        };
        let is_form = matches!(
            (*dom).get(candidate).map(|n| &n.data),
            Some(dom::NodeData::Element { tag, .. }) if tag == "form"
        );
        if is_form {
            break candidate;
        }
        current = (*dom).get(candidate).and_then(|n| n.parent);
    };

    let class_id = node_class_id_for(ctx, dom, form_id);
    let form_obj = node_object(ctx, class_id, form_id);
    match action {
        ButtonAction::Submit => {
            form_request_submit(ctx, form_obj, 0, std::ptr::null_mut());
        }
        ButtonAction::Reset => {
            form_reset(ctx, form_obj, 0, std::ptr::null_mut());
        }
    }
    sys::JS_FreeValue(ctx, form_obj);
}

/// Defines `HTMLFormElement`-specific properties: `elements` (an
/// HTMLCollection of this form's controls), real `reset()`/`requestSubmit()`
/// (see [`form_reset`]/[`form_request_submit`]'s own docs), and a still-noop
/// `submit()` — per spec `submit()` doesn't fire a `"submit"` event or run
/// constraint validation either (unlike `requestSubmit()`), so the *only*
/// thing missing from it is the actual network request/navigation this
/// engine has no page-navigation-from-script layer to perform; existing
/// real-world form code calling it just doesn't throw.
pub(super) unsafe fn define_form_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let cname = CString::new("elements").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(form_elements_get),
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

    let submit_name = CString::new("submit").unwrap();
    let submit_fn = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<sys::JSCFunction, sys::JSCFunction>(element_scroll_noop),
        submit_name.as_ptr(),
        0,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, proto, submit_name.as_ptr(), submit_fn);

    for (name, func) in [
        ("reset", form_reset as sys::JSCFunction),
        ("requestSubmit", form_request_submit as sys::JSCFunction),
    ] {
        let cname = CString::new(name).unwrap();
        let value = sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), 0, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, proto, cname.as_ptr(), value);
    }
}

unsafe extern "C" fn form_elements_get(
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
    let controls = descendants_matching_tags(&*dom, id, &["input", "select", "textarea", "button"]);
    html_collection(ctx, controls)
}

/// Real `HTMLFormElement.reset()`: dispatches a cancelable `"reset"` event
/// on the form first (per spec order) and, unless canceled, restores every
/// `<input>`/`<textarea>` descendant's live `.value` to its `.defaultValue`
/// (the `value` attribute — see `content::define_default_value`'s doc),
/// and every checkbox/radio `<input>`'s live `.checked` to its
/// `.defaultChecked` (the `checked` attribute — see
/// `content::define_default_checked`'s doc). Restoring goes straight
/// through `dom::Dom::set_value`/`set_checked`, not the JS `value`/
/// `checked` setters, so it does *not* fire an `"input"`/`"change"` event
/// per control — matches real `reset()`, which only fires its own one
/// `"reset"` event, not a cascade of others.
///
/// Also restores every `<option>` descendant's live `.selected` to its
/// `.defaultSelected` (the `selected` attribute — see
/// `content::define_default_selected`'s doc) — real per-option state,
/// unlike `<input>`/`<textarea>` which reset per-control.
unsafe extern "C" fn form_reset(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    if !crate::events::dispatch(ctx, this_val, "reset") {
        return sys::js_undefined();
    }
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    for control in descendants_matching_tags(&*dom, id, &["input", "textarea"]) {
        let is_checkable = matches!(
            (*dom).attribute(control, "type"),
            Some("checkbox") | Some("radio")
        );
        if is_checkable {
            let default_checked = (*dom).attribute(control, "checked").is_some();
            (*dom).set_checked(control, default_checked);
        } else {
            let default = (*dom)
                .attribute(control, "value")
                .unwrap_or_default()
                .to_string();
            (*dom).set_value(control, &default);
        }
    }
    for option in descendants_matching_tags(&*dom, id, &["option"]) {
        let default_selected = (*dom).attribute(option, "selected").is_some();
        (*dom).set_selected(option, default_selected);
    }
    sys::js_undefined()
}

/// Real `HTMLFormElement.requestSubmit()`: per spec, interactively
/// validates every named control first (real `checkValidity()`, reused
/// directly rather than duplicated — each failing control gets its own
/// real `"invalid"` event, same as calling `checkValidity()` on it
/// directly would); if every control is valid, dispatches a real
/// cancelable `"submit"` event on the form. Whether or not anything
/// listens or cancels it, no actual network request/navigation happens —
/// same documented gap `submit()` already has (no page-navigation-from-
/// script layer in this engine), the real, spec-shaped part here is the
/// validation-gate-then-event sequencing a script can observe and act on.
unsafe extern "C" fn form_request_submit(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    let node_class = crate::class_registry::class_id_for(
        sys::JS_GetRuntime(ctx),
        super::node_registry::NODE_CLASS_KIND,
    );
    let mut all_valid = true;
    for control in descendants_matching_tags(&*dom, id, &["input", "select", "textarea"]) {
        let control_obj = super::node_registry::node_object(ctx, node_class, control);
        let result = check_validity(ctx, control_obj, 0, std::ptr::null_mut());
        let valid = result.tag == sys::JS_TAG_BOOL && result.u.int32 != 0;
        sys::JS_FreeValue(ctx, control_obj);
        if !valid {
            all_valid = false;
        }
    }
    if all_valid {
        crate::events::dispatch(ctx, this_val, "submit");
    }
    sys::js_undefined()
}
