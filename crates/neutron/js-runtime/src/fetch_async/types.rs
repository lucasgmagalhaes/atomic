//! Pending-fetch/XHR bookkeeping types + the per-context state map — split
//! out from `fetch_async/mod.rs`.

use std::sync::mpsc;

use quickjs_sys as sys;

pub(super) struct PendingFetch {
    pub(super) resolve: sys::JSValue,
    pub(super) reject: sys::JSValue,
    pub(super) url: String,
    pub(super) receiver: mpsc::Receiver<Result<net::Response, net::Error>>,
}

/// A fully-owned request description handed to the background thread —
/// nothing in it borrows from JS-land. Fields are read by `fetch_sync`
/// too, so they're crate-visible.
pub(crate) struct RequestSpec {
    pub(crate) method: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Option<Vec<u8>>,
}

pub(super) struct PendingXhr {
    pub(super) xhr_obj: sys::JSValue,
    pub(super) url: String,
    pub(super) receiver: mpsc::Receiver<Result<net::Response, net::Error>>,
}

#[derive(Default)]
pub(super) struct AsyncState {
    pub(super) fetches: Vec<PendingFetch>,
    pub(super) xhrs: Vec<PendingXhr>,
}

thread_local! {
    // Keyed by JSContext pointer, same reasoning as `timers::REGISTRIES`:
    // avoids needing an unsafe Send/Sync impl just to hold `!Send`
    // `JSValue`s behind a plain static.
    pub(super) static STATE: std::cell::RefCell<std::collections::HashMap<usize, AsyncState>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

pub(super) fn with_state<R>(ctx: *mut sys::JSContext, f: impl FnOnce(&mut AsyncState) -> R) -> R {
    STATE.with(|s| {
        let mut map = s.borrow_mut();
        let state = map.entry(ctx as usize).or_default();
        f(state)
    })
}
