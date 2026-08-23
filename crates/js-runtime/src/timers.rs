//! `setTimeout`/`setInterval`/`clearTimeout`/`clearInterval` and
//! `requestAnimationFrame`/`cancelAnimationFrame`.
//!
//! Deviation from the spec: these are normally driven by a real per-tab
//! event loop tied to the OS timer queue and the display's vsync — that
//! loop doesn't exist yet (`profile`/`ipc` spawn a worker process, but
//! nothing inside it runs one; see `CLAUDE.md`'s phase 2/4 notes on the
//! same gap blocking `performance.now`'s per-document origin). Until then
//! this is a manual, cooperative pump: [`crate::Context::run_pending_timers`]
//! must be called by the host for any due timer or queued animation-frame
//! callback to actually fire. One call = "one tick": every timer whose
//! delay has elapsed fires once (rescheduling if it's an interval), and
//! every pending `requestAnimationFrame` callback fires and is cleared —
//! there's no real per-frame cadence, just whatever the host calls this at.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::os::raw::c_int;
use std::time::{Duration, Instant};

use quickjs_sys as sys;

struct Timer {
    id: u32,
    callback: sys::JSValue,
    interval: Option<Duration>,
    fire_at: Instant,
}

struct RafCallback {
    id: u32,
    callback: sys::JSValue,
}

#[derive(Default)]
struct TimerState {
    next_id: u32,
    timers: Vec<Timer>,
    rafs: Vec<RafCallback>,
    // Ids cleared while their timer was mid-fire (removed from `timers` for
    // the duration of its own callback, so a self-`clearInterval` wouldn't
    // otherwise find it) — checked by the reschedule step in `pump`.
    cancelled: HashSet<u32>,
}

thread_local! {
    // Keyed by JSContext pointer (as usize) rather than living on `Context`
    // itself: `Context` is deliberately !Send/!Sync (raw-pointer field), and
    // a thread_local sidesteps needing an unsafe Send/Sync impl just to
    // stash JSValue (itself !Send/!Sync via its raw-pointer union member)
    // behind a plain static. Every native fn below and `Context::drop`
    // funnel through here on the same thread that owns the context.
    static REGISTRIES: RefCell<HashMap<usize, TimerState>> = RefCell::new(HashMap::new());
}

fn with_state<R>(ctx: *mut sys::JSContext, f: impl FnOnce(&mut TimerState) -> R) -> R {
    REGISTRIES.with(|reg| {
        let mut map = reg.borrow_mut();
        let state = map.entry(ctx as usize).or_insert_with(TimerState::default);
        f(state)
    })
}

unsafe fn read_js_number(v: sys::JSValue) -> f64 {
    match v.tag {
        sys::JS_TAG_FLOAT64 => v.u.float64,
        sys::JS_TAG_INT => v.u.int32 as f64,
        _ => 0.0,
    }
}

fn duration_from_ms(ms: f64) -> Duration {
    Duration::from_secs_f64(ms.max(0.0) / 1000.0)
}

unsafe fn schedule(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
    interval: bool,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let callback = sys::JS_DupValue(ctx, *argv);
    let delay_ms = if argc >= 2 {
        read_js_number(*argv.add(1))
    } else {
        0.0
    };
    let duration = duration_from_ms(delay_ms);
    let id = with_state(ctx, |s| {
        s.next_id += 1;
        let id = s.next_id;
        s.timers.push(Timer {
            id,
            callback,
            interval: if interval { Some(duration) } else { None },
            fire_at: Instant::now() + duration,
        });
        id
    });
    sys::js_float64(id as f64)
}

unsafe extern "C" fn set_timeout(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    schedule(ctx, argc, argv, false)
}

unsafe extern "C" fn set_interval(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    schedule(ctx, argc, argv, true)
}

unsafe fn clear_timer(ctx: *mut sys::JSContext, argc: c_int, argv: *mut sys::JSValue) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let id = read_js_number(*argv) as u32;
    with_state(ctx, |s| {
        if let Some(pos) = s.timers.iter().position(|t| t.id == id) {
            let timer = s.timers.remove(pos);
            sys::JS_FreeValue(ctx, timer.callback);
        } else {
            // Might be mid-fire (removed from `timers` while its callback
            // runs) — flag it so `pump` skips rescheduling it.
            s.cancelled.insert(id);
        }
    });
    sys::js_undefined()
}

unsafe extern "C" fn clear_timeout(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    clear_timer(ctx, argc, argv)
}

unsafe extern "C" fn clear_interval(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    clear_timer(ctx, argc, argv)
}

unsafe extern "C" fn request_animation_frame(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let callback = sys::JS_DupValue(ctx, *argv);
    let id = with_state(ctx, |s| {
        s.next_id += 1;
        let id = s.next_id;
        s.rafs.push(RafCallback { id, callback });
        id
    });
    sys::js_float64(id as f64)
}

unsafe extern "C" fn cancel_animation_frame(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let id = read_js_number(*argv) as u32;
    with_state(ctx, |s| {
        if let Some(pos) = s.rafs.iter().position(|r| r.id == id) {
            let raf = s.rafs.remove(pos);
            sys::JS_FreeValue(ctx, raf.callback);
        }
    });
    sys::js_undefined()
}

/// Registers the six globals on `ctx`.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);

    let add = |name: &str, func: sys::JSCFunction, length: c_int| {
        let name_c = CString::new(name).unwrap();
        let f = sys::JS_NewCFunction2(ctx, func, name_c.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, global, name_c.as_ptr(), f);
    };

    add("setTimeout", set_timeout, 2);
    add("setInterval", set_interval, 2);
    add("clearTimeout", clear_timeout, 1);
    add("clearInterval", clear_interval, 1);
    add("requestAnimationFrame", request_animation_frame, 1);
    add("cancelAnimationFrame", cancel_animation_frame, 1);

    sys::JS_FreeValue(ctx, global);
}

/// Runs every timer whose delay has elapsed (rescheduling intervals) and
/// every queued animation-frame callback, then clears the frame queue.
/// Returns how many callbacks fired. See the module doc for why this must
/// be called explicitly rather than firing on a real clock.
pub(crate) unsafe fn pump(ctx: *mut sys::JSContext) -> usize {
    let now = Instant::now();

    let due_timers = with_state(ctx, |s| {
        let mut due = Vec::new();
        let mut i = 0;
        while i < s.timers.len() {
            if s.timers[i].fire_at <= now {
                due.push(s.timers.remove(i));
            } else {
                i += 1;
            }
        }
        due
    });

    let due_rafs = with_state(ctx, |s| std::mem::take(&mut s.rafs));

    let mut fired = 0;
    for timer in due_timers {
        let result = sys::JS_Call(ctx, timer.callback, sys::js_undefined(), 0, std::ptr::null_mut());
        sys::JS_FreeValue(ctx, result);
        fired += 1;

        match timer.interval {
            Some(interval) => {
                let cancelled = with_state(ctx, |s| s.cancelled.remove(&timer.id));
                if cancelled {
                    sys::JS_FreeValue(ctx, timer.callback);
                } else {
                    with_state(ctx, |s| {
                        s.timers.push(Timer {
                            id: timer.id,
                            callback: timer.callback,
                            interval: Some(interval),
                            fire_at: Instant::now() + interval,
                        });
                    });
                }
            }
            None => {
                sys::JS_FreeValue(ctx, timer.callback);
            }
        }
    }

    for raf in due_rafs {
        let result = sys::JS_Call(ctx, raf.callback, sys::js_undefined(), 0, std::ptr::null_mut());
        sys::JS_FreeValue(ctx, result);
        sys::JS_FreeValue(ctx, raf.callback);
        fired += 1;
    }

    fired
}

/// Frees every still-owned callback for `ctx` and drops its registry entry.
/// Must run before `JS_FreeContext(ctx)` — `JS_FreeValue` needs a live
/// context.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    let state = REGISTRIES.with(|reg| reg.borrow_mut().remove(&(ctx as usize)));
    let Some(state) = state else { return };
    for timer in state.timers {
        sys::JS_FreeValue(ctx, timer.callback);
    }
    for raf in state.rafs {
        sys::JS_FreeValue(ctx, raf.callback);
    }
}
