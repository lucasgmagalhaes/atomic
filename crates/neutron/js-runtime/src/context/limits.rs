//! Per-eval time budget + memory limit — split out from `context/mod.rs`.

use quickjs_sys as sys;

use crate::script_limits;

use super::Context;

impl<'rt> Context<'rt> {
    /// Bounds how long each [`Context::eval`] call may run: once the
    /// budget passes, quickjs's interrupt handler aborts the running
    /// script with an "interrupted" InternalError at its next
    /// loop/function-entry check, surfacing as `eval`'s usual `Err` (see
    /// [`script_limits`] for scope: per eval call, not per timer/promise
    /// callback). Installing a budget is idempotent and cheap; pass
    /// shorter/longer durations freely, use [`Context::clear_time_budget`]
    /// to lift it.
    pub fn set_time_budget(&mut self, budget: std::time::Duration) {
        unsafe { script_limits::install(self.ptr) };
        self._time_budget = Some(budget);
    }

    /// Lifts a budget installed by [`Context::set_time_budget`] - later
    /// evals run unbounded again (the handler stays installed but never
    /// interrupts without a deadline).
    pub fn clear_time_budget(&mut self) {
        if self._time_budget.take().is_some() {
            script_limits::set_deadline(self.ptr, None);
        }
    }

    /// Caps the runtime's total JS-heap allocation at `bytes` (quickjs-ng's
    /// `JS_SetMemoryLimit`): an allocation over the cap throws a RangeError
    /// into whatever script asked for it instead of succeeding. Runtime-wide,
    /// which here means per-context - every context owns its whole runtime.
    pub fn set_memory_limit(&self, bytes: usize) {
        unsafe { sys::JS_SetMemoryLimit(sys::JS_GetRuntime(self.ptr), bytes) };
    }
}
