//! Window/`BrowsingContext` identity (`ROADMAP.md` items 35 iframe, 36
//! multiple browsing contexts, 37 cross-window messaging;
//! `.claude/plans/architecture-p5-foundations.plan.md` Stage 3).
//!
//! Today `profile-worker`'s `Page` hardcodes exactly one `Context` + one
//! `dom::Dom` per OS process, with no notion of "more than one document"
//! at all — the real architectural gap those three items share. This
//! module is deliberately narrow: it only gives every real page `Context`
//! a process-wide-unique [`WindowId`] and a cross-context message inbox
//! keyed by it, proven with one round-trip test. It does **not** yet
//! implement `iframe`, `window.open`, or `window.postMessage`/a real
//! `"message"` event/`Window` object — each of those is its own future
//! item, built on top of this primitive rather than inventing a parallel
//! one (`architecture/overview.md`'s "architecture before APIs" rule).
//!
//! # Why a separate inbox from `message_channel`
//!
//! `message_channel`'s `MessagePort` registry is keyed by `ctx as usize`
//! and its `Port.object`/`peer` are ids scoped to *one* `JSContext` — apt
//! for same-realm delivery (both ports always live in the same `Context`)
//! but structurally unable to cross a `JSContext`/`JSRuntime` boundary: a
//! live `JSValue` belongs to the context that made it. Cross-window
//! messaging needs the data itself to survive that boundary, so this
//! module reuses `value_bridge::js_to_storage_value`/
//! `value_bridge::storage_value_to_js` — the same context-independent
//! `storage::value::Value` wire shape `message_channel` already reuses
//! internally for its own same-realm clone — as the thing that actually
//! crosses.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use quickjs_sys as sys;
use storage::value::Value;

/// Process-wide-unique identity for a live window/`BrowsingContext`.
/// Never reused within a process (unlike `dom::NodeId`'s
/// generation-recycled slots) — a window's identity must stay stable and
/// unambiguous for as long as any other window might still hold a
/// reference to it (a future `iframe.contentWindow`/`window.open` handle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(u64);

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    // `profile-worker` runs one Context per OS process/thread today, but
    // several BrowsingContexts sharing one thread (a future same-process
    // iframe) will all register here — keyed by WindowId, not by `ctx`,
    // unlike `message_channel`'s own registry.
    static INBOXES: RefCell<HashMap<WindowId, Vec<Value>>> = RefCell::new(HashMap::new());
}

/// Registers a new window identity with an empty inbox. Called once by
/// [`crate::Context::with_dom`] — a plain `Context::new` without a `dom`
/// has no window identity at all, same "no host state behind it" degrade
/// this crate already uses for every other `HostState`-backed feature.
pub(crate) fn register() -> WindowId {
    let id = WindowId(NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed));
    INBOXES.with(|m| {
        m.borrow_mut().insert(id, Vec::new());
    });
    id
}

/// Frees `id`'s inbox — called from `Context::drop`. Any message still
/// queued for a window that's gone is simply dropped, matching a real
/// closed window never receiving a late `postMessage`.
pub(crate) fn unregister(id: WindowId) {
    INBOXES.with(|m| {
        m.borrow_mut().remove(&id);
    });
}

/// Real cross-window `postMessage`'s data-transfer half: clones `data`
/// out of `from_ctx` into a context-independent `storage::value::Value`
/// and queues it in `to`'s inbox. Returns `false` if `to` isn't a
/// currently registered window (already closed, or never existed) — same
/// "unreachable target, message silently dropped" semantics
/// `message_channel::port_post_message` already has for a closed peer.
pub(crate) unsafe fn send(from_ctx: *mut sys::JSContext, to: WindowId, data: sys::JSValue) -> bool {
    let value = crate::value_bridge::js_to_storage_value(from_ctx, data);
    INBOXES.with(|m| match m.borrow_mut().get_mut(&to) {
        Some(inbox) => {
            inbox.push(value);
            true
        }
        None => false,
    })
}

/// Rehydrates every message queued for `id` (via [`send`]) as a live
/// `JSValue` in `ctx` — `ctx` must be the `Context` that owns window
/// identity `id`, same "pump your own queue" contract every other
/// pump-based primitive in this crate (`timers`/`fetch_async`/
/// `mutation_observer`/`message_channel`) already has. Appended, in
/// order, onto `globalThis.__crossWindowInbox` (created as an empty array
/// on first delivery if absent) rather than a real `"message"`
/// event/`Window` object, since neither exists yet — a documented, honest
/// scope cut: ROADMAP item 37 (Cross-window messaging) replaces this
/// internal global with a real `MessageEvent` dispatch once
/// `window.postMessage` itself is implemented on top of this primitive.
/// Returns how many messages were delivered.
pub(crate) unsafe fn pump(ctx: *mut sys::JSContext, id: WindowId) -> usize {
    let inbox = INBOXES.with(|m| {
        m.borrow_mut()
            .get_mut(&id)
            .map(std::mem::take)
            .unwrap_or_default()
    });
    if inbox.is_empty() {
        return 0;
    }
    let global = sys::JS_GetGlobalObject(ctx);
    let prop = b"__crossWindowInbox\0";
    let mut arr = sys::JS_GetPropertyStr(ctx, global, prop.as_ptr() as *const _);
    if !sys::JS_IsArray(arr) {
        sys::JS_FreeValue(ctx, arr);
        arr = sys::JS_NewArray(ctx);
        sys::JS_SetPropertyStr(
            ctx,
            global,
            prop.as_ptr() as *const _,
            sys::JS_DupValue(ctx, arr),
        );
    }
    let mut len: i64 = 0;
    sys::JS_GetLength(ctx, arr, &mut len);
    let mut delivered = 0usize;
    for value in inbox {
        let js_value = crate::value_bridge::storage_value_to_js(ctx, &value);
        sys::JS_SetPropertyUint32(ctx, arr, (len as u32) + delivered as u32, js_value);
        delivered += 1;
    }
    sys::JS_FreeValue(ctx, arr);
    sys::JS_FreeValue(ctx, global);
    delivered
}
