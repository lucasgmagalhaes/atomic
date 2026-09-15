//! @spec atomicjs-profiling#modular-execution-core
//! Optional profiling and Tier-1 dispatch accounting.

use crate::tier_one::TierOneFunction;
use crate::value::FunctionFeedback;

pub(super) trait FeedbackSink {
    fn on_function_call(&mut self, function_index: usize);
    fn on_loop_backedge(&mut self, function_index: usize);

    #[inline(always)]
    fn on_loop_backedges(&mut self, function_index: usize, count: u32) {
        for _ in 0..count {
            self.on_loop_backedge(function_index);
        }
    }
}

pub(super) struct NoFeedback;

impl FeedbackSink for NoFeedback {
    #[inline(always)]
    fn on_function_call(&mut self, _: usize) {}

    #[inline(always)]
    fn on_loop_backedge(&mut self, _: usize) {}

    #[inline(always)]
    fn on_loop_backedges(&mut self, _: usize, _: u32) {}
}

pub(super) struct CollectingFeedback<'a> {
    pub(super) entries: &'a mut [FunctionFeedback],
}

impl FeedbackSink for CollectingFeedback<'_> {
    #[inline(always)]
    fn on_function_call(&mut self, function_index: usize) {
        self.entries[function_index].call_count += 1;
    }

    #[inline(always)]
    fn on_loop_backedge(&mut self, function_index: usize) {
        self.entries[function_index].loop_count += 1;
    }

    #[inline(always)]
    fn on_loop_backedges(&mut self, function_index: usize, count: u32) {
        self.entries[function_index].loop_count = self.entries[function_index]
            .loop_count
            .saturating_add(count);
    }
}

pub(super) struct TierOneState<'a> {
    pub(super) functions: Option<&'a [Option<TierOneFunction>]>,
    pub(super) calls: &'a mut u32,
    pub(super) fallbacks: &'a mut u32,
}
