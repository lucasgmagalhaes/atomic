//! `history` — real same-document navigation state (`pushState`/
//! `replaceState`/`back`/`forward`/`go`/`length`/`state`), backed by a
//! plain JS array kept as a hidden `__entries` own property on the
//! `history` object itself (each entry `{state, url, title}`), plus a
//! `__index` pointer into it — same "store native-ish state as plain JS
//! values on the object" convention `events.rs`'s listener records use,
//! rather than a separate native side table.
//!
//! `state` is real structured-cloned (via [`crate::value_bridge::deep_clone`])
//! before being stored — mutating the object passed to `pushState`/
//! `replaceState` afterward is not visible through `history.state`/a later
//! `popstate`, matching a real browser's independent snapshot. Same scope
//! cut `value_bridge`'s own doc carries (no `Date`/`Map`/`Set`/typed
//! arrays/`RegExp`, no cycle detection) — not a new one introduced here.
//!
//! `back`/`forward`/`go` only ever move within *this context's own*
//! `pushState`/`replaceState` stack — real browser history that predates
//! the current page load (whatever the host navigated through before this
//! script ran) isn't modeled, since nothing hands this crate that list.
//!
//! Split into `helpers.rs` (shared low-level property/read helpers),
//! `push_replace.rs` (`pushState`/`replaceState`), `navigate.rs`
//! (`back`/`forward`/`go`), and `accessors.rs` (`length`/`state` getters).

mod accessors;
mod helpers;
mod navigate;
mod push_replace;

use std::ffi::CString;

use quickjs_sys as sys;

use accessors::{define_readonly, length_get, state_get};
use helpers::make_entry;
use navigate::{back, forward, go};
use push_replace::{push_state, replace_state};

const ENTRIES_PROP: &[u8] = b"__entries\0";
const INDEX_PROP: &[u8] = b"__index\0";
const MAX_ENTRIES: i64 = 1000;

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let history = sys::JS_NewObject(ctx);

    let initial_entries = sys::JS_NewArray(ctx);
    let initial_entry = make_entry(ctx, sys::js_null(), sys::js_null(), "");
    sys::JS_SetPropertyUint32(ctx, initial_entries, 0, initial_entry);
    helpers::set_prop(ctx, history, ENTRIES_PROP, initial_entries);
    helpers::set_prop(ctx, history, INDEX_PROP, sys::js_float64(0.0));

    define_readonly(
        ctx,
        history,
        "length",
        length_get as crate::js_helpers::Getter,
    );
    define_readonly(
        ctx,
        history,
        "state",
        state_get as crate::js_helpers::Getter,
    );

    for (name, func, arity) in [
        ("pushState", push_state as sys::JSCFunction, 3),
        ("replaceState", replace_state as sys::JSCFunction, 3),
        ("back", back as sys::JSCFunction, 0),
        ("forward", forward as sys::JSCFunction, 0),
        ("go", go as sys::JSCFunction, 1),
    ] {
        let cname = CString::new(name).unwrap();
        sys::JS_SetPropertyStr(
            ctx,
            history,
            cname.as_ptr(),
            sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), arity, sys::JS_CFUNC_GENERIC, 0),
        );
    }

    let global = sys::JS_GetGlobalObject(ctx);
    let name = CString::new("history").unwrap();
    sys::JS_SetPropertyStr(ctx, global, name.as_ptr(), history);
    sys::JS_FreeValue(ctx, global);
}
