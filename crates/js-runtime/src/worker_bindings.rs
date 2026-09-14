//! Real JS-facing `Worker` (`spec/matrix/runtime.md`'s "Worker runtime"
//! line — the actual remaining gap identified after correcting a prior
//! overclaim: `crates/workers/`'s `Worker` is real OS-thread
//! infrastructure, but nothing registered it as a JS global, so no page
//! script could ever call `new Worker(...)`).
//!
//! `new Worker(scriptText)` spawns a genuine OS thread (`thread::spawn`,
//! same shape `crates/workers::Worker::spawn` already proves sound for
//! this engine) that builds its own `sys::JSRuntime`/`sys::JSContext`
//! entirely inside the thread closure — never touched from any other
//! thread, so no `Send`/`Sync` bound on the runtime/context pointers is
//! needed. Scope cut, deliberate: `scriptText` is literal JS source, not
//! a URL to fetch — this crate's fetch pipeline lives at `profile-worker`
//! level, not reachable from inside a native binding (same cut
//! `crates/workers::Worker::spawn` already documents).
//!
//! # Structured clone across the thread boundary
//!
//! Message data crosses via `storage::value::Value` — plain owned Rust
//! data (String/Number/Vec/HashMap/Bytes, no embedded `JSValue`), so it's
//! `Send` for free and needs no `unsafe impl`. `postMessage` on either
//! side clones the argument with [`crate::value_bridge::js_to_storage_value`]
//! *before* pushing it onto an `mpsc::Sender<Value>`; the receiving side
//! rehydrates with [`crate::value_bridge::storage_value_to_js`] once
//! delivered. This is a real upgrade over `crates/workers`'s own
//! hand-escaped-string wire format, reusing the exact `Value` shape
//! `window_registry`'s cross-window `postMessage` already established as
//! this crate's `Send`-safe wire type.
//!
//! # Delivery cadence
//!
//! Worker -> main delivery follows this crate's "host pumps explicitly,
//! no real event loop yet" convention (same as `timers`/`fetch_async`/
//! `window_registry`): [`pump`] drains every live worker's outbox and
//! dispatches a real `"message"` event on that `Worker` instance's own
//! JS object, called from [`crate::Context::run_pending_timers`]. Main ->
//! worker delivery is push-based instead: the worker thread blocks on
//! `mpsc::Receiver::recv` between messages, so a `postMessage` from the
//! main thread wakes it immediately rather than waiting on a pump of its
//! own — there is no separate "worker pumps its own inbox" step, since a
//! worker's only job while idle is to wait for its next message.
//!
//! # Scope cuts (documented, not silently missing)
//!
//! No `SharedWorker` (a separate, larger primitive — a worker instance
//! shared across multiple connecting contexts via `onconnect`/`.port` —
//! left for a follow-up now that a real single-owner `Worker` exists to
//! build on). No nested workers (a worker's own thread never registers
//! this module, so `new Worker(...)` inside a worker script throws
//! `ReferenceError` same as any other unregistered global). No
//! `importScripts`. No Transferable objects on the worker-postMessage
//! path yet (`message_channel.rs`'s own `Uint8Array` transfer stays
//! same-realm-only for now — a real cross-thread transfer would need
//! `Value::Bytes` to carry the detach signal too, left as future work).
//! Uncaught exceptions inside a worker's message handler are logged to
//! stderr and otherwise swallowed — no real `Worker.onerror`/`"error"`
//! event yet (a documented, narrower-than-full-spec cut, same shape this
//! crate already accepts for other partial features).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread;

use quickjs_sys as sys;
use storage::value::Value;

const WORKER_ID_PROP: &[u8] = b"__worker_id\0";
const ONMESSAGE_PROP: &[u8] = b"__onmessage\0";

static NEXT_WORKER_ID: AtomicU32 = AtomicU32::new(1);

struct WorkerEntry {
    to_worker: mpsc::Sender<Value>,
    from_worker: mpsc::Receiver<Value>,
    join: Option<thread::JoinHandle<()>>,
    /// The `Worker` instance's own JS object, dup'd — [`pump`] dispatches
    /// `"message"` events directly onto it.
    object: sys::JSValue,
}

thread_local! {
    // Keyed by owning ctx (the realm that called `new Worker(...)`), same
    // "outer map by ctx" convention `message_channel`/`window_registry`
    // already use — a `Worker` instance is only ever driven by the
    // context that created it (no cross-context sharing here, unlike a
    // future `SharedWorker`).
    static WORKERS: RefCell<HashMap<usize, HashMap<u32, WorkerEntry>>> = RefCell::new(HashMap::new());
}

// Set once, at the very start of a worker's own OS thread, before its
// script evaluates — every native binding running on that thread reads
// this instead of needing an opaque pointer thread-local storage can't
// carry across the `JS_SetContextOpaque` boundary this module doesn't use
// (a worker context has no `HostState`, only this channel).
thread_local! {
    static WORKER_OUTBOX: RefCell<Option<mpsc::Sender<Value>>> = const { RefCell::new(None) };
    static WORKER_CLOSED: Cell<bool> = const { Cell::new(false) };
}

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &[u8]) -> sys::JSValue {
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr() as *const _)
}
unsafe fn set_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &[u8], value: sys::JSValue) {
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr() as *const _, value);
}

unsafe fn worker_id(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> Option<u32> {
    let value = get_prop(ctx, this_val, WORKER_ID_PROP);
    let id = match value.tag {
        sys::JS_TAG_INT => Some(value.u.int32 as u32),
        sys::JS_TAG_FLOAT64 => Some(value.u.float64 as u32),
        _ => None,
    };
    sys::JS_FreeValue(ctx, value);
    id
}

/// Drains every completed quickjs Promise job on the calling thread's own
/// runtime/context — same loop `fetch_async::pump` already uses, needed
/// here too since a worker's own script can use promises/async functions
/// independently of the main thread's job queue.
unsafe fn drain_jobs(ctx: *mut sys::JSContext) {
    let rt = sys::JS_GetRuntime(ctx);
    let mut job_ctx = ctx;
    while sys::JS_IsJobPending(rt) {
        if sys::JS_ExecutePendingJob(rt, &mut job_ctx) <= 0 {
            break;
        }
    }
}

unsafe fn log_exception(ctx: *mut sys::JSContext) {
    let exception = sys::JS_GetException(ctx);
    if exception.tag == sys::JS_TAG_NULL {
        return;
    }
    let mut len: usize = 0;
    let c_str_ptr = sys::JS_ToCStringLen2(ctx, &mut len, exception, false);
    sys::JS_FreeValue(ctx, exception);
    if !c_str_ptr.is_null() {
        let text = std::ffi::CStr::from_ptr(c_str_ptr).to_string_lossy();
        eprintln!("[worker] uncaught exception: {text}");
        sys::JS_FreeCString(ctx, c_str_ptr);
    }
}

/// Real `self.postMessage`/global `postMessage` inside a worker's own
/// thread — clones the argument and pushes it onto this thread's own
/// outbox, delivered to the main thread the next time its own
/// [`crate::Context::run_pending_timers`] calls [`pump`].
unsafe extern "C" fn worker_self_post_message(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let data = if argc >= 1 {
        *argv
    } else {
        sys::js_undefined()
    };
    let cloned = crate::value_bridge::js_to_storage_value(ctx, data);
    WORKER_OUTBOX.with(|cell| {
        if let Some(tx) = cell.borrow().as_ref() {
            let _ = tx.send(cloned);
        }
    });
    sys::js_undefined()
}

unsafe extern "C" fn worker_self_close(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    WORKER_CLOSED.with(|c| c.set(true));
    sys::js_undefined()
}

unsafe extern "C" fn worker_self_onmessage_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, ONMESSAGE_PROP)
}
unsafe extern "C" fn worker_self_onmessage_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    set_prop(ctx, this_val, ONMESSAGE_PROP, sys::JS_DupValue(ctx, val));
    sys::js_undefined()
}

/// The worker thread's own body: builds a fresh `Runtime`/`Context`
/// (deliberately not [`crate::Context::new`] — a worker realm needs only
/// a handful of globals, not this crate's whole DOM-less-but-still-heavy
/// `register_standard_globals` surface, and pulling in DOM-adjacent
/// globals like `document.cookie` would misrepresent a worker's real
/// scope), evaluates `script`, then blocks on its inbox until the main
/// thread drops the sender (real `Worker.terminate()`) or the script
/// itself calls `close()`.
fn worker_thread_main(script: String, inbox: mpsc::Receiver<Value>, outbox: mpsc::Sender<Value>) {
    unsafe {
        let rt = sys::JS_NewRuntime();
        assert!(!rt.is_null(), "JS_NewRuntime returned null");
        let ctx = sys::JS_NewContext(rt);
        assert!(!ctx.is_null(), "JS_NewContext returned null");

        WORKER_OUTBOX.with(|cell| *cell.borrow_mut() = Some(outbox));

        crate::console::register(ctx);
        crate::crypto::register(ctx);
        crate::timers::register(ctx);
        crate::url_bindings::register(ctx);
        crate::events::register(ctx);
        crate::event_subclasses::register(ctx);
        crate::value_bridge::register(ctx);

        let global = sys::JS_GetGlobalObject(ctx);
        crate::events::define_simple_event_target(ctx, global);
        crate::js_helpers::define_method(ctx, global, "postMessage", worker_self_post_message, 1);
        crate::js_helpers::define_method(ctx, global, "close", worker_self_close, 0);
        crate::js_helpers::define_getter_setter(
            ctx,
            global,
            "onmessage",
            worker_self_onmessage_get,
            worker_self_onmessage_set,
        );
        // Real `self` (a `DedicatedWorkerGlobalScope`'s own IDL alias to
        // itself) - lets worker script use the real, spec-idiomatic
        // `self.postMessage`/`self.onmessage` form, not just the bare
        // global one.
        set_prop(ctx, global, b"self\0", sys::JS_DupValue(ctx, global));

        let src_c = CString::new(script).unwrap_or_default();
        let name_c = CString::new("<worker>").unwrap();
        let result = sys::JS_Eval(
            ctx,
            src_c.as_ptr(),
            src_c.as_bytes().len(),
            name_c.as_ptr(),
            sys::JS_EVAL_TYPE_GLOBAL as i32,
        );
        if sys::js_is_exception(&result) {
            log_exception(ctx);
        }
        sys::JS_FreeValue(ctx, result);
        drain_jobs(ctx);

        while !WORKER_CLOSED.with(|c| c.get()) {
            let Ok(value) = inbox.recv() else {
                break; // sender dropped: real Worker.terminate()
            };
            let data = crate::value_bridge::storage_value_to_js(ctx, &value);
            let event = crate::events::create_event(ctx, "message", false, false);
            if sys::js_is_exception(&event) {
                sys::JS_FreeValue(ctx, data);
                continue;
            }
            set_prop(ctx, event, b"data\0", data);
            let event_for_dispatch = sys::JS_DupValue(ctx, event);
            crate::events::dispatch_event_object(ctx, global, event_for_dispatch);

            let handler = get_prop(ctx, global, ONMESSAGE_PROP);
            if sys::JS_IsFunction(ctx, handler) {
                let mut arg = sys::JS_DupValue(ctx, event);
                let call_result = sys::JS_Call(ctx, handler, global, 1, &mut arg);
                if sys::js_is_exception(&call_result) {
                    log_exception(ctx);
                }
                sys::JS_FreeValue(ctx, call_result);
                sys::JS_FreeValue(ctx, arg);
            }
            sys::JS_FreeValue(ctx, handler);
            sys::JS_FreeValue(ctx, event);
            drain_jobs(ctx);
        }

        sys::JS_FreeValue(ctx, global);
        crate::timers::cleanup(ctx);
        WORKER_OUTBOX.with(|cell| *cell.borrow_mut() = None);
        sys::JS_FreeContext(ctx);
        sys::JS_FreeRuntime(rt);
    }
}

unsafe extern "C" fn worker_constructor(
    ctx: *mut sys::JSContext,
    _new_target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        let msg = sys::JS_NewStringLen(ctx, b"Worker".as_ptr() as *const _, 6);
        return sys::JS_Throw(ctx, msg);
    }
    let mut len: usize = 0;
    let c_str_ptr = sys::JS_ToCStringLen2(ctx, &mut len, *argv, false);
    if c_str_ptr.is_null() {
        return sys::js_exception();
    }
    let script_bytes = std::slice::from_raw_parts(c_str_ptr as *const u8, len);
    let script = String::from_utf8_lossy(script_bytes).into_owned();
    sys::JS_FreeCString(ctx, c_str_ptr);

    let (to_worker_tx, to_worker_rx) = mpsc::channel::<Value>();
    let (from_worker_tx, from_worker_rx) = mpsc::channel::<Value>();
    let join = thread::spawn(move || worker_thread_main(script, to_worker_rx, from_worker_tx));

    let id = NEXT_WORKER_ID.fetch_add(1, Ordering::Relaxed);
    let obj = sys::JS_NewObject(ctx);
    set_prop(ctx, obj, WORKER_ID_PROP, sys::js_float64(id as f64));
    crate::events::define_simple_event_target(ctx, obj);
    crate::js_helpers::define_method(ctx, obj, "postMessage", worker_post_message, 1);
    crate::js_helpers::define_method(ctx, obj, "terminate", worker_terminate, 0);
    crate::js_helpers::define_getter_setter(
        ctx,
        obj,
        "onmessage",
        worker_onmessage_get,
        worker_onmessage_set,
    );

    WORKERS.with(|reg| {
        reg.borrow_mut().entry(ctx as usize).or_default().insert(
            id,
            WorkerEntry {
                to_worker: to_worker_tx,
                from_worker: from_worker_rx,
                join: Some(join),
                object: sys::JS_DupValue(ctx, obj),
            },
        );
    });

    obj
}

unsafe extern "C" fn worker_post_message(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = worker_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let data = if argc >= 1 {
        *argv
    } else {
        sys::js_undefined()
    };
    let cloned = crate::value_bridge::js_to_storage_value(ctx, data);
    WORKERS.with(|reg| {
        if let Some(entry) = reg.borrow().get(&(ctx as usize)).and_then(|m| m.get(&id)) {
            let _ = entry.to_worker.send(cloned);
        }
    });
    sys::js_undefined()
}

/// Real `Worker.terminate()`: drops this worker's sender half, which
/// wakes its blocked `recv()` with a disconnect error and ends the
/// thread's loop — then joins it so the OS thread is fully gone before
/// this call returns (matches `crates/workers`'s own `Drop` join
/// convention, just triggered explicitly rather than on drop).
unsafe extern "C" fn worker_terminate(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    if let Some(id) = worker_id(ctx, this_val) {
        let entry = WORKERS.with(|reg| {
            reg.borrow_mut()
                .get_mut(&(ctx as usize))
                .and_then(|m| m.remove(&id))
        });
        if let Some(mut entry) = entry {
            drop(entry.to_worker);
            if let Some(join) = entry.join.take() {
                let _ = join.join();
            }
            sys::JS_FreeValue(ctx, entry.object);
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn worker_onmessage_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, ONMESSAGE_PROP)
}
unsafe extern "C" fn worker_onmessage_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    set_prop(ctx, this_val, ONMESSAGE_PROP, sys::JS_DupValue(ctx, val));
    sys::js_undefined()
}

/// Delivers every message queued in every live worker's outbox — called
/// from [`crate::Context::run_pending_timers`], same cadence every other
/// pump-based primitive in this crate already has. Non-blocking: uses
/// `try_recv` so a worker that hasn't posted anything yet never stalls
/// the main thread's frame loop. Returns how many messages were
/// delivered.
pub(crate) unsafe fn pump(ctx: *mut sys::JSContext) -> usize {
    let mut delivered = 0usize;
    let ids: Vec<u32> = WORKERS.with(|reg| {
        reg.borrow()
            .get(&(ctx as usize))
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default()
    });
    for id in ids {
        loop {
            let received = WORKERS.with(|reg| {
                reg.borrow()
                    .get(&(ctx as usize))
                    .and_then(|m| m.get(&id))
                    .map(|entry| entry.from_worker.try_recv())
            });
            let Some(Ok(value)) = received else { break };
            let object = WORKERS.with(|reg| {
                reg.borrow()
                    .get(&(ctx as usize))
                    .and_then(|m| m.get(&id))
                    .map(|entry| sys::JS_DupValue(ctx, entry.object))
            });
            let Some(object) = object else { break };

            let data = crate::value_bridge::storage_value_to_js(ctx, &value);
            let event = crate::events::create_event(ctx, "message", false, false);
            if sys::js_is_exception(&event) {
                sys::JS_FreeValue(ctx, data);
                sys::JS_FreeValue(ctx, object);
                continue;
            }
            set_prop(ctx, event, b"data\0", data);
            let event_for_dispatch = sys::JS_DupValue(ctx, event);
            crate::events::dispatch_event_object(ctx, object, event_for_dispatch);

            let handler = get_prop(ctx, object, ONMESSAGE_PROP);
            if sys::JS_IsFunction(ctx, handler) {
                let mut arg = sys::JS_DupValue(ctx, event);
                let call_result = sys::JS_Call(ctx, handler, object, 1, &mut arg);
                sys::JS_FreeValue(ctx, call_result);
                sys::JS_FreeValue(ctx, arg);
            }
            sys::JS_FreeValue(ctx, handler);
            sys::JS_FreeValue(ctx, event);
            sys::JS_FreeValue(ctx, object);
            delivered += 1;
        }
    }
    delivered
}

/// Terminates and frees every worker `ctx` owns — must run before
/// `JS_FreeContext(ctx)`, same ordering requirement every other module's
/// `cleanup(ctx)` already documents. Real "close with the page" semantics
/// (a real browser also terminates a page's own workers when the page
/// unloads).
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    let entries = WORKERS.with(|reg| reg.borrow_mut().remove(&(ctx as usize)));
    if let Some(entries) = entries {
        for (_, mut entry) in entries {
            drop(entry.to_worker);
            if let Some(join) = entry.join.take() {
                let _ = join.join();
            }
            sys::JS_FreeValue(ctx, entry.object);
        }
    }
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let ctor_name = CString::new("Worker").unwrap();
    let ctor = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<
            unsafe extern "C" fn(
                *mut sys::JSContext,
                sys::JSValue,
                c_int,
                *mut sys::JSValue,
            ) -> sys::JSValue,
            sys::JSCFunction,
        >(worker_constructor),
        ctor_name.as_ptr(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, ctor_name.as_ptr(), ctor);
    sys::JS_FreeValue(ctx, global);
}
