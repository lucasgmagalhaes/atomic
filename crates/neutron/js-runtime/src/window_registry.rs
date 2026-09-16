//! Window/`BrowsingContext` identity and real multi-context support
//! (`ROADMAP.md` items 36 multiple browsing contexts, 37 cross-window
//! messaging; `.claude/plans/architecture-p5-foundations.plan.md`
//! Stage 3).
//!
//! Every real page `Context` (`Context::with_dom`) gets a process-wide-
//! unique [`WindowId`] and a cross-context message inbox keyed by it.
//! `window.open()` (`open_window`) builds a genuinely independent second
//! `JSContext` sharing the calling window's `JSRuntime` — its own global
//! object, its own blank `dom::Dom`, its own `WindowId` — registered here
//! as a *child* of whichever `Context` opened it. A dynamically-opened
//! window has no host loop of its own driving its timers/fetches/
//! cross-window inbox, so [`pump_children`] piggybacks it onto its
//! opener's own [`crate::Context::run_pending_timers`] call, recursively —
//! the whole tree of windows a page opens gets driven by the same
//! per-frame pump the top-level host already makes, with no changes
//! needed to that host loop itself.
//!
//! Delivered messages dispatch a real `"message"` event (via
//! `events::dispatch_event_object`, same shape `message_channel`'s own
//! same-realm delivery already uses) onto the target window's global
//! object, not an internal test-only global — this closes the scope cut
//! this module's own Stage-3 version left open.
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
//!
//! # `<iframe>` (`ROADMAP.md` item 35): `.contentWindow` only
//!
//! `window.open()` creates a new *top-level* browsing context (real spec
//! behavior: `top`/`parent`/`self` on the opened window all alias itself,
//! exactly like `window.rs`'s existing aliasing already does for any
//! `Context` — no changes needed there). `<iframe>.contentWindow`
//! (`dom_bindings::iframe`) reuses this exact primitive — `window_for_
//! iframe` opens (and caches by the `<iframe>`'s own `NodeId`) a child
//! window the same way `open_window` does. What's still deliberately
//! not attempted: `contentDocument` requires exposing a live object from
//! a *different* `JSContext` directly into the parent's own realm — this
//! engine has no cross-`JSContext` object-sharing mechanism, only the
//! structured-clone-shaped message passing `postMessage` already uses —
//! and real visual embedding needs `layout-engine` to lay out and paint
//! a second document nested inside a box, which it has no concept of.
//! Both are honest, separate future work.

use std::cell::RefCell;
use std::collections::HashMap;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicU64, Ordering};

use quickjs_sys as sys;
use storage::value::Value;

/// Process-wide-unique identity for a live window/`BrowsingContext`.
/// Never reused within a process (unlike `dom::NodeId`'s
/// generation-recycled slots) — a window's identity must stay stable and
/// unambiguous for as long as any other window might still hold a
/// reference to it (a `window.open()` handle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(u64);

impl WindowId {
    /// For encoding into a JS-visible hidden property (`window.rs`'s
    /// `RemoteWindow` object carries one as a plain `float64`, same
    /// `JS_TAG_FLOAT64`-encoded-id pattern `message_channel::PORT_ID_PROP`
    /// already uses for its own `u32` port ids). Lossless in practice —
    /// a process would need to open quadrillions of windows before a
    /// `u64 -> f64` round trip could lose precision.
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }

    pub(crate) fn from_u64(v: u64) -> Self {
        WindowId(v)
    }
}

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

/// A `JSContext` this module created and owns (via `open_window`), kept
/// alive here rather than behind a safe `Context<'rt>` — a dynamically
/// opened window has no `&'rt Runtime` borrow to attach a `Context` to,
/// since it's created from inside a native callback that only has a raw
/// `JSContext`/`JSRuntime` pointer on hand.
struct OwnedChild {
    ctx: *mut sys::JSContext,
    // Kept alive for the child's lifetime (mirrors `Context::_host_state`'s
    // own doc: moving the Box moves this field, not the heap allocation
    // `JS_SetContextOpaque` points at).
    _host_state: Box<crate::host_state::HostState>,
}

thread_local! {
    // `profile-worker` runs one Context per OS process/thread today, but
    // several BrowsingContexts sharing one thread (`window.open()`) all
    // register here — keyed by WindowId, not by `ctx`, unlike
    // `message_channel`'s own registry.
    static INBOXES: RefCell<HashMap<WindowId, Vec<Value>>> = RefCell::new(HashMap::new());
    // Windows this module itself created (and therefore owns/must free) —
    // absent for a window belonging to an externally-owned `Context`
    // (e.g. the top-level page `profile-worker` constructs).
    static OWNED_CHILDREN: RefCell<HashMap<WindowId, OwnedChild>> = RefCell::new(HashMap::new());
    // Opener ctx pointer -> every WindowId it opened, for `pump_children`
    // to walk without needing every opener to track its own children.
    static CHILDREN_OF: RefCell<HashMap<usize, Vec<WindowId>>> = RefCell::new(HashMap::new());
    // (owner ctx, iframe NodeId) -> the WindowId `window_for_iframe`
    // lazily opened for it, so repeated `.contentWindow` reads return the
    // *same* window instead of opening a fresh one every time.
    static IFRAME_WINDOWS: RefCell<HashMap<(usize, dom::NodeId), WindowId>> =
        RefCell::new(HashMap::new());
    // (viewing ctx, target WindowId) -> the RemoteWindow object already
    // built for that pair, so repeated access (`iframe.contentWindow`
    // read twice, `window.open()`'s handle looked up again) returns the
    // *same* JS object identity — real spec behavior for `contentWindow`
    // in particular. Same per-(ctx, id) object-identity-cache shape
    // `dom_bindings::node_registry::NODE_OBJECTS` already uses for
    // `dom::NodeId`s.
    static REMOTE_WINDOW_OBJECTS: RefCell<HashMap<(usize, WindowId), sys::JSValue>> =
        RefCell::new(HashMap::new());
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

/// Rehydrates every message queued for `id` (via [`send`]) and dispatches
/// a real `"message"` event for each, in order, on `ctx`'s global object —
/// `ctx` must be the `Context` that owns window identity `id`, same
/// "pump your own queue" contract every other pump-based primitive in
/// this crate (`timers`/`fetch_async`/`mutation_observer`/
/// `message_channel`) already has. Returns how many messages were
/// delivered.
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
    let mut delivered = 0usize;
    for value in inbox {
        let data = crate::value_bridge::storage_value_to_js(ctx, &value);
        let event = crate::events::create_event(ctx, "message", false, false);
        if sys::js_is_exception(&event) {
            sys::JS_FreeValue(ctx, data);
            continue;
        }
        let data_prop = b"data\0";
        sys::JS_SetPropertyStr(ctx, event, data_prop.as_ptr() as *const _, data);
        // `dispatch_event_object` consumes `event` but *not* `target` (see
        // its own doc) — `global` stays borrowed across every iteration,
        // freed once after the loop, no per-iteration dup needed.
        crate::events::dispatch_event_object(ctx, global, event);
        delivered += 1;
    }
    sys::JS_FreeValue(ctx, global);
    delivered
}

/// Real `window.open(url)`'s core: builds a genuinely independent
/// `JSContext` sharing `owner_ctx`'s `JSRuntime` (own global object, own
/// blank `dom::Dom`, own `WindowId`), registers it as a child of
/// `owner_ctx` for [`pump_children`], and returns its `WindowId`. `url`
/// is accepted but not fetched/navigated — a documented scope cut (this
/// crate's navigation pipeline is `profile-worker`-level, not reachable
/// from inside a native binding); the new window starts as a real, blank,
/// scriptable document, same as a real `about:blank` popup before it
/// navigates anywhere.
pub(crate) unsafe fn open_window(owner_ctx: *mut sys::JSContext) -> WindowId {
    let rt = sys::JS_GetRuntime(owner_ctx);
    let ptr = sys::JS_NewContext(rt);
    assert!(!ptr.is_null(), "JS_NewContext returned null");
    crate::context::register_standard_globals(ptr);

    let mut host_state = crate::context::build_host_state(dom::Dom::new());
    let id = host_state.window.id;
    let raw = host_state.as_mut() as *mut crate::host_state::HostState as *mut std::os::raw::c_void;
    sys::JS_SetContextOpaque(ptr, raw);
    crate::dom_bindings::register(ptr);
    crate::computed_style::register(ptr);

    OWNED_CHILDREN.with(|m| {
        m.borrow_mut().insert(
            id,
            OwnedChild {
                ctx: ptr,
                _host_state: host_state,
            },
        );
    });
    CHILDREN_OF.with(|m| {
        m.borrow_mut()
            .entry(owner_ctx as usize)
            .or_default()
            .push(id);
    });

    // Real `opener`: set unconditionally here (not in the JS-facing
    // `window.open()` wrapper) so it's consistent whether a window was
    // opened via that global or directly via `Context::open_window` (the
    // Rust-level entry point tests/hosts use). A plain `Context::new`
    // caller without a `dom` has no window id, so its opened windows get
    // no `opener` — same degrade-gracefully convention every other
    // `HostState`-backed feature in this crate already follows.
    let owner_state = crate::host_state::get(owner_ctx);
    if !owner_state.is_null() {
        let owner_id = (*owner_state).window.id;
        let opener = make_remote_window(ptr, owner_id);
        let new_global = sys::JS_GetGlobalObject(ptr);
        let prop = b"opener\0";
        sys::JS_SetPropertyStr(ptr, new_global, prop.as_ptr() as *const _, opener);
        sys::JS_FreeValue(ptr, new_global);
    }

    id
}

const WINDOW_ID_PROP: &[u8] = b"__window_id\0";

/// Builds a `RemoteWindow` object for `target` — a plain object (no
/// native class, same "no quickjs class/opaque pointer" shape
/// `abort_controller`/`message_channel` already use) carrying `target`'s
/// id plus real `postMessage`/`close`. Callable in any `Context`
/// regardless of which window it represents, since [`send`]/
/// [`close_window`] take a `WindowId` directly rather than needing to be
/// called from a specific realm.
pub(crate) unsafe fn make_remote_window(
    ctx: *mut sys::JSContext,
    target: WindowId,
) -> sys::JSValue {
    let cache_key = (ctx as usize, target);
    if let Some(cached) = REMOTE_WINDOW_OBJECTS.with(|m| m.borrow().get(&cache_key).copied()) {
        return sys::JS_DupValue(ctx, cached);
    }
    let obj = sys::JS_NewObject(ctx);
    sys::JS_SetPropertyStr(
        ctx,
        obj,
        WINDOW_ID_PROP.as_ptr() as *const _,
        sys::js_float64(target.as_u64() as f64),
    );
    crate::js_helpers::define_method(ctx, obj, "postMessage", remote_window_post_message, 1);
    crate::js_helpers::define_method(ctx, obj, "close", remote_window_close, 0);
    REMOTE_WINDOW_OBJECTS.with(|m| {
        m.borrow_mut().insert(cache_key, sys::JS_DupValue(ctx, obj));
    });
    obj
}

unsafe fn read_window_id(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> Option<WindowId> {
    let prop = sys::JS_GetPropertyStr(ctx, this_val, WINDOW_ID_PROP.as_ptr() as *const _);
    let id = match prop.tag {
        sys::JS_TAG_FLOAT64 => Some(WindowId::from_u64(prop.u.float64 as u64)),
        sys::JS_TAG_INT => Some(WindowId::from_u64(prop.u.int32 as u64)),
        _ => None,
    };
    sys::JS_FreeValue(ctx, prop);
    id
}

unsafe extern "C" fn remote_window_post_message(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(target) = read_window_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let data = if argc >= 1 {
        *argv
    } else {
        sys::js_undefined()
    };
    send(ctx, target, data);
    sys::js_undefined()
}

unsafe extern "C" fn remote_window_close(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    if let Some(target) = read_window_id(ctx, this_val) {
        close_window(target);
    }
    sys::js_undefined()
}

/// Pumps every window `owner_ctx` opened via [`open_window`] (recursively,
/// so grandchildren opened by a child get pumped too) — see this module's
/// own doc for why a dynamically-opened window has no host loop of its
/// own. Returns how many timers/fetches/messages/etc. fired across the
/// whole subtree.
pub(crate) unsafe fn pump_children(owner_ctx: *mut sys::JSContext) -> usize {
    let children = CHILDREN_OF
        .with(|m| m.borrow().get(&(owner_ctx as usize)).cloned())
        .unwrap_or_default();
    let mut total = 0;
    for child_id in children {
        let Some(child_ptr) = OWNED_CHILDREN.with(|m| m.borrow().get(&child_id).map(|c| c.ctx))
        else {
            continue; // already closed
        };
        total += pump(child_ptr, child_id);
        total += crate::timers::pump(child_ptr);
        total += crate::fetch_async::pump(child_ptr);
        total += crate::mutation_observer::pump(child_ptr);
        total += crate::message_channel::pump(child_ptr);
        total += crate::module_loader::pump(child_ptr);
        total += pump_children(child_ptr);
    }
    total
}

/// Real `window.close()`'s core for a dynamically-opened window: runs
/// every module's cleanup, frees the `JSContext`, and drops its owned
/// `HostState`. No-op (returns `false`) for a `WindowId` this module
/// doesn't own (an externally-owned top-level `Context`, or an already-
/// closed window) — a page can't close a window it didn't open via
/// `window.open()`, matching real `Window.close()`'s own "only closes
/// windows opened by script" restriction.
pub(crate) unsafe fn close_window(id: WindowId) -> bool {
    let Some(child) = OWNED_CHILDREN.with(|m| m.borrow_mut().remove(&id)) else {
        return false;
    };
    close_children_of(child.ctx);
    cleanup_remote_window_cache(child.ctx);
    crate::module_loader::cleanup(child.ctx);
    crate::context::cleanup_standard_globals(child.ctx);
    crate::dom_bindings::cleanup(child.ctx);
    sys::JS_FreeContext(child.ctx);
    unregister(id);
    true
}

/// Closes every window `owner_ctx` opened, recursively — called from
/// [`crate::Context::drop`] so a page's whole window tree closes with it
/// (real spec: closing a window's opener doesn't auto-close windows it
/// opened, but this crate has no independent host loop to keep orphaned
/// windows alive without their opener ever pumping them again, so
/// leaking them would be strictly worse than closing them).
pub(crate) unsafe fn close_children_of(owner_ctx: *mut sys::JSContext) {
    let children = CHILDREN_OF
        .with(|m| m.borrow_mut().remove(&(owner_ctx as usize)))
        .unwrap_or_default();
    for child_id in children {
        close_window(child_id);
    }
}

/// Real `<iframe>.contentWindow`'s core (`ROADMAP.md` item 35): returns
/// the child window already opened for `iframe_id` on `ctx`, or opens
/// (and caches) one on first access — same underlying primitive as
/// `window.open()`, just keyed by the `<iframe>` element's own `NodeId`
/// instead of created fresh on every call. See `dom_bindings::iframe`'s
/// own doc for the `.contentDocument`/visual-embedding scope cut.
pub(crate) unsafe fn window_for_iframe(
    ctx: *mut sys::JSContext,
    iframe_id: dom::NodeId,
) -> WindowId {
    let key = (ctx as usize, iframe_id);
    if let Some(existing) = IFRAME_WINDOWS.with(|m| m.borrow().get(&key).copied()) {
        return existing;
    }
    let id = open_window(ctx);
    IFRAME_WINDOWS.with(|m| {
        m.borrow_mut().insert(key, id);
    });
    id
}

/// Frees every cached `RemoteWindow` object built *in* `ctx` (regardless
/// of which target window each represents) — must run before `ctx` is
/// freed, same ordering requirement every other module's own
/// `cleanup(ctx)` documents. Called from both `Context::drop` (a
/// top-level context) and [`close_window`] (a dynamically-opened one).
pub(crate) unsafe fn cleanup_remote_window_cache(ctx: *mut sys::JSContext) {
    let keys: Vec<(usize, WindowId)> = REMOTE_WINDOW_OBJECTS.with(|m| {
        m.borrow()
            .keys()
            .filter(|(owner, _)| *owner == ctx as usize)
            .copied()
            .collect()
    });
    for key in keys {
        if let Some(obj) = REMOTE_WINDOW_OBJECTS.with(|m| m.borrow_mut().remove(&key)) {
            sys::JS_FreeValue(ctx, obj);
        }
    }
}

/// The raw `JSContext` pointer for a window this module opened —
/// `None` for an externally-owned window (the top-level page) or a
/// closed/nonexistent one. Used by the `RemoteWindow` JS binding
/// (`window_bindings`) to know whether `.close()` applies.
pub(crate) fn context_ptr_for(id: WindowId) -> Option<*mut sys::JSContext> {
    OWNED_CHILDREN.with(|m| m.borrow().get(&id).map(|c| c.ctx))
}
