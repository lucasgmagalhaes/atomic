//! `CompositionEvent` — split out from `mod.rs`, same convention every
//! other subclass in this module already uses. Real IME composition
//! surface (`spec/matrix/events.md` line 17's last item): `.data` is the
//! only field beyond base `Event` this engine models (real spec also has
//! `.locale`, unused by any real page this engine's own test/mockup
//! surface exercises - scope cut, matches `event_subclasses.rs`'s own
//! precedent of trimming rarely-used fields).

use quickjs_sys as sys;
use std::os::raw::c_int;

use super::{
    constructor_prototype, options_arg, read_bool_option, read_string_option, read_type_arg,
    set_string_prop,
};

pub(super) unsafe extern "C" fn composition_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    let Some(data) = read_string_option(ctx, options, "data", "") else {
        return sys::js_exception();
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        return event;
    }
    set_string_prop(ctx, event, "data", &data);
    let proto = constructor_prototype(ctx, "CompositionEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}
