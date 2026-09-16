//! `AbortController`/`AbortSignal` (`ROADMAP.md` P2 item 17): real
//! cancellation-signaling objects, wired into `addEventListener`'s
//! previously-no-op `signal` option (`events::listeners`).
//!
//! Neither class stores native/opaque state — both are plain `JS_NewObject`
//! instances with a real prototype (`JS_SetPrototype`, resolved from the
//! constructor's own `.prototype`, same convention `blob.rs`'s
//! `resolve_prototype` established) carrying hidden `__aborted`/`__reason`/
//! `__signal_listeners` own properties, same "state as plain JS values on
//! the object" convention `history.rs`'s `__entries`/`__index` and
//! `listeners.rs`'s per-record objects already use. `AbortSignal` reuses
//! [`crate::events::define_simple_event_target`] for its `addEventListener`/
//! `dispatchEvent("abort")` surface — it's exactly the "plain JS object,
//! not a `Node`" shape that helper already exists for (`window`/`document`/
//! `history`).
//!
//! `signal` tracking for `addEventListener`: rather than a native closure
//! (this crate's `extern "C" fn` callbacks can't capture Rust state),
//! `track_listener` pushes a plain `{node, type, callback, capture}` record
//! onto the signal's own `__signal_listeners` array — the same shape
//! `listeners.rs`'s own per-`(node, type)` records already use. `fire_abort`
//! walks that array and calls `events::remove_matching_record` for each
//! entry, so a listener registered with `{signal}` is really removed when
//! the controller aborts, not just marked.
//!
//! Real scope cuts (documented, not silent): `new AbortSignal()` throws
//! (matches real browsers — only `AbortSignal.abort()`/an `AbortController`
//! ever produce one); no `AbortSignal.timeout()`/`AbortSignal.any()`; no
//! `.onabort` IDL attribute (only `addEventListener("abort", ...)` — this
//! crate has no generic `on<type>` property convention for any
//! `EventTarget`, not one newly cut here); and `fetch`/`XMLHttpRequest`
//! don't yet read a `signal` option themselves (`ROADMAP.md`'s explicit
//! call-out was `addEventListener`'s `signal`, wired here — request
//! cancellation is a separate, larger follow-up since this crate's
//! in-flight fetches run on a real OS thread with no cancellation channel
//! yet).
//!
//! Split into `helpers.rs` (shared JS string/property helpers and
//! `resolve_prototype`), `signal.rs` (`AbortSignal`, `signal_aborted`,
//! `track_listener`, `fire_abort`), and `controller.rs`
//! (`AbortController`) — this file keeps the public `register` entry
//! point.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::events;

mod controller;
mod helpers;
mod signal;

pub(crate) use signal::{signal_aborted, track_listener};

use controller::{controller_abort, controller_constructor, controller_signal_get};
use signal::{
    signal_abort_static, signal_aborted_get, signal_constructor, signal_reason_get,
    signal_throw_if_aborted,
};

/// Registers the global `AbortController` and `AbortSignal` constructors.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);

    let signal_proto = sys::JS_NewObject(ctx);
    crate::js_helpers::define_getter(ctx, signal_proto, "aborted", signal_aborted_get);
    crate::js_helpers::define_getter(ctx, signal_proto, "reason", signal_reason_get);
    crate::js_helpers::define_method(
        ctx,
        signal_proto,
        "throwIfAborted",
        signal_throw_if_aborted,
        0,
    );
    events::define_simple_event_target(ctx, signal_proto);

    let signal_ctor_name = CString::new("AbortSignal").unwrap();
    let signal_ctor = sys::JS_NewCFunction2(
        ctx,
        signal_constructor,
        signal_ctor_name.as_ptr(),
        0,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    crate::js_helpers::define_method(ctx, signal_ctor, "abort", signal_abort_static, 1);
    let proto_name = CString::new("prototype").unwrap();
    // Consumes `signal_proto` directly (no `JS_DupValue`) — this is its
    // only owning reference (unlike `blob.rs`'s `ensure_blob_class`, there's
    // no `JS_SetClassProto` here to hand a second one to; this crate skips
    // `class_registry` entirely for these two classes, see the module doc).
    sys::JS_SetPropertyStr(ctx, signal_ctor, proto_name.as_ptr(), signal_proto);
    sys::JS_SetPropertyStr(ctx, global, signal_ctor_name.as_ptr(), signal_ctor);

    let controller_proto = sys::JS_NewObject(ctx);
    crate::js_helpers::define_getter(ctx, controller_proto, "signal", controller_signal_get);
    crate::js_helpers::define_method(ctx, controller_proto, "abort", controller_abort, 0);

    let controller_ctor_name = CString::new("AbortController").unwrap();
    let controller_ctor = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<
            unsafe extern "C" fn(
                *mut sys::JSContext,
                sys::JSValue,
                c_int,
                *mut sys::JSValue,
            ) -> sys::JSValue,
            sys::JSCFunction,
        >(controller_constructor),
        controller_ctor_name.as_ptr(),
        0,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    // Same reasoning as `signal_proto` above — consumed directly.
    sys::JS_SetPropertyStr(ctx, controller_ctor, proto_name.as_ptr(), controller_proto);
    sys::JS_SetPropertyStr(ctx, global, controller_ctor_name.as_ptr(), controller_ctor);

    sys::JS_FreeValue(ctx, global);
}
