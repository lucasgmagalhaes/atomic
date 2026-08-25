//! Time budgets and memory quotas for untrusted script execution - the
//! "a hostile page must not be able to hang or OOM its host" half of the
//! security story (JS_ENGINE_CAPABILITY_MATRIX.md section 1). Two knobs:
//!
//! - [`crate::Context::set_time_budget`]: installs quickjs-ng's
//!   interrupt handler and stamps a deadline at the start of every
//!   `Context::eval`; once it passes, the interpreter aborts the running
//!   script with an "interrupted" InternalError at its next
//!   loop/function-entry check. The budget applies per eval call, not to
//!   timers/promise jobs pumped later via `run_pending_timers` (those
//!   are host-scheduled callbacks; bounding each of those individually
//!   is future work once there's a real event loop to own it).
//!
//! - [`crate::Context::set_memory_limit`]: quickjs-ng's runtime-wide
//!   allocation cap. Over-limit allocations from inside running script
//!   throw a RangeError instead of succeeding, surfacing through `eval`
//!   like any other exception. Runtime-wide by nature (quickjs counts
//!   allocations against the JSRuntime), which is exactly right in this
//!   workspace: every context owns its whole runtime.
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_int, c_void};
use std::time::Instant;

use quickjs_sys as sys;

/// What the installed interrupt handler reads: the deadline stamped by
/// the next budgeted `eval`, or none while no budgeted script is running
/// (the handler then never interrupts).
struct BudgetState {
    deadline: Option<Instant>,
}

thread_local! {
    // Keyed by JSContext pointer (as usize) - same shape as timers'/
    // fetch_async's registries: `Context` is deliberately !Send/!Sync,
    // and everything touching this map runs on the thread owning the
    // context.
    static BUDGETS: RefCell<HashMap<usize, BudgetState>> = RefCell::new(HashMap::new());
}

unsafe extern "C" fn interrupt_handler(_rt: *mut sys::JSRuntime, opaque: *mut c_void) -> c_int {
    let ctx = opaque as *mut sys::JSContext;
    BUDGETS.with(|reg| {
        match reg.borrow().get(&(ctx as usize)).and_then(|s| s.deadline) {
            Some(deadline) => (Instant::now() >= deadline) as c_int,
            None => 0,
        }
    })
}

pub(crate) unsafe fn install(ctx: *mut sys::JSContext) {
    BUDGETS.with(|reg| {
        let mut map = reg.borrow_mut();
        let state = map.entry(ctx as usize).or_insert(BudgetState { deadline: None });
        // Re-installing must not resurrect a stale deadline.
        state.deadline = None;
    });
    sys::JS_SetInterruptHandler(sys::JS_GetRuntime(ctx), Some(interrupt_handler), ctx as *mut c_void);
}

/// Stamps (or clears) the deadline the interrupt handler checks while one
/// `eval` call runs.
pub(crate) fn set_deadline(ctx: *mut sys::JSContext, deadline: Option<Instant>) {
    BUDGETS.with(|reg| {
        if let Some(state) = reg.borrow_mut().get_mut(&(ctx as usize)) {
            state.deadline = deadline;
        }
    });
}

/// Unregisters the handler and drops the state (context teardown) - a
/// stale map entry would otherwise be read if a new context ever landed
/// on the same address.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    let present = BUDGETS.with(|reg| reg.borrow_mut().remove(&(ctx as usize)).is_some());
    if present {
        sys::JS_SetInterruptHandler(sys::JS_GetRuntime(ctx), None, std::ptr::null_mut());
    }
}
