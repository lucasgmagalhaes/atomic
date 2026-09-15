//! @spec atomicjs-profiling#tier-one-policy
//! Pause-aware, memory-bounded tiering policy. This module intentionally makes
//! decisions only: Tier 0 remains the interpreter until an executable Tier 1
//! is installed by a later isolated milestone.

use crate::value::FunctionFeedback;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostState {
    Running,
    Paused,
}

#[derive(Debug, Clone, Copy)]
pub struct TieringPolicy {
    pub call_threshold: u32,
    pub loop_threshold: u32,
    pub code_budget_bytes: usize,
    pub enabled: bool,
}

impl Default for TieringPolicy {
    fn default() -> Self {
        Self {
            call_threshold: 1_000,
            loop_threshold: 10_000,
            code_budget_bytes: 64 * 1024,
            enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierDecision {
    Interpret,
    Eligible,
}

/// Persistent per-program tiering state. It accumulates opt-in feedback
/// across isolated executions; execution remains in Tier 0 until a later
/// executable Tier 1 consumes `Eligible` decisions.
pub struct TieringController {
    policy: TieringPolicy,
    host: HostState,
    feedback: Vec<FunctionFeedback>,
    /// Admission is stateful: once a hot function has reserved part of the
    /// Tier 1 budget, later functions must account for that reservation too.
    /// The executable backend will replace this bookkeeping with its actual
    /// code cache, but it must preserve the same cap semantics.
    admitted: Vec<bool>,
    admitted_bytes: usize,
}

impl TieringController {
    pub fn new(policy: TieringPolicy, function_count: usize) -> Self {
        Self {
            policy,
            host: HostState::Running,
            feedback: (0..function_count)
                .map(|_| FunctionFeedback::default())
                .collect(),
            admitted: vec![false; function_count],
            admitted_bytes: 0,
        }
    }

    pub fn set_host_state(&mut self, host: HostState) {
        self.host = host;
    }

    pub fn observe(
        &mut self,
        run_feedback: &[FunctionFeedback],
        estimated_code_bytes: &[usize],
    ) -> Vec<TierDecision> {
        assert_eq!(self.feedback.len(), run_feedback.len());
        assert_eq!(self.feedback.len(), estimated_code_bytes.len());
        self.feedback
            .iter_mut()
            .zip(run_feedback)
            .for_each(|(total, run)| {
                total.call_count = total.call_count.saturating_add(run.call_count);
                total.loop_count = total.loop_count.saturating_add(run.loop_count);
            });
        if self.host == HostState::Paused || !self.policy.enabled {
            // A paused host never retains tiered work. Clearing this before
            // decisions makes the next invocation deterministic even when a
            // pause happened between two calls.
            self.admitted.fill(false);
            self.admitted_bytes = 0;
            return vec![TierDecision::Interpret; self.feedback.len()];
        }

        self.feedback
            .iter()
            .zip(estimated_code_bytes)
            .enumerate()
            .map(|(index, (feedback, estimate))| {
                if self.admitted[index]
                    || self
                        .policy
                        .decide(self.host, feedback, self.admitted_bytes, *estimate)
                        == TierDecision::Eligible
                {
                    if !self.admitted[index] {
                        self.admitted[index] = true;
                        self.admitted_bytes = self.admitted_bytes.saturating_add(*estimate);
                    }
                    TierDecision::Eligible
                } else {
                    TierDecision::Interpret
                }
            })
            .collect()
    }
}

impl TieringPolicy {
    pub fn decide(
        self,
        host: HostState,
        feedback: &FunctionFeedback,
        installed_bytes: usize,
        estimated_code_bytes: usize,
    ) -> TierDecision {
        if !self.enabled
            || host == HostState::Paused
            || installed_bytes.saturating_add(estimated_code_bytes) > self.code_budget_bytes
        {
            return TierDecision::Interpret;
        }
        if feedback.call_count >= self.call_threshold || feedback.loop_count >= self.loop_threshold
        {
            TierDecision::Eligible
        } else {
            TierDecision::Interpret
        }
    }
}
