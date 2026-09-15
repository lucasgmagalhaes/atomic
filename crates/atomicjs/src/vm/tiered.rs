//! @spec atomicjs-profiling#modular-execution-core
//! Tier-1 admission and lifecycle, separate from interpreter dispatch.

use crate::error::AtomicJsError;
use crate::tier_one::TierOneFunction;
use crate::tiering::{HostState, TierDecision, TieringController, TieringPolicy};
use crate::value::Value;

use super::CompiledProgram;

#[derive(Debug)]
pub struct TieredRun {
    pub value: Value,
    pub decisions: Vec<TierDecision>,
    pub tier_one_installs: u32,
    pub tier_one_calls: u32,
    pub tier_one_fallbacks: u32,
}

pub struct TieredProgram {
    program: CompiledProgram,
    controller: TieringController,
    estimated_code_bytes: Vec<usize>,
    supported: Vec<bool>,
    candidates: Vec<Option<TierOneFunction>>,
    groups: Vec<Option<Vec<usize>>>,
    tier_one: Vec<Option<TierOneFunction>>,
}

impl TieredProgram {
    pub fn compile(source: &str, policy: TieringPolicy) -> Result<Self, AtomicJsError> {
        let program = CompiledProgram::compile(source)?;
        let mut candidates: Vec<Option<TierOneFunction>> = program
            .module
            .functions
            .iter()
            .map(TierOneFunction::compile)
            .collect();
        let leaf_catalog = candidates.clone();
        candidates
            .iter_mut()
            .flatten()
            .for_each(|candidate| candidate.inline_leaf_calls(&leaf_catalog));
        let individual_bytes = candidates
            .iter()
            .map(|candidate| {
                candidate
                    .as_ref()
                    .map_or(0, TierOneFunction::estimated_bytes)
            })
            .collect::<Vec<_>>();
        let (supported, estimated_code_bytes, groups) = call_groups(&candidates, &individual_bytes);
        let function_count = program.module.functions.len();
        Ok(Self {
            program,
            controller: TieringController::new(policy, function_count),
            estimated_code_bytes,
            supported,
            candidates,
            groups,
            tier_one: vec![None; function_count],
        })
    }

    pub fn set_host_state(&mut self, host: HostState) {
        self.controller.set_host_state(host);
        if host == HostState::Paused {
            self.tier_one.fill(None);
        }
    }

    pub fn run(&mut self) -> Result<TieredRun, AtomicJsError> {
        let mut tier_one_calls = 0;
        let mut tier_one_fallbacks = 0;
        let (value, feedback) = self.program.run_with_feedback_and_tier_one(
            Some(&self.tier_one),
            &mut tier_one_calls,
            &mut tier_one_fallbacks,
        )?;
        let decisions =
            self.controller
                .observe(&feedback, &self.estimated_code_bytes, &self.supported);
        let mut tier_one_installs = 0;
        for (index, decision) in decisions.iter().enumerate() {
            if *decision == TierDecision::Eligible && self.tier_one[index].is_none() {
                for &member in self.groups[index]
                    .as_ref()
                    .expect("eligible Tier-1 root must own a call group")
                {
                    if self.tier_one[member].is_none() {
                        self.tier_one[member] = self.candidates[member].clone();
                        tier_one_installs += u32::from(self.tier_one[member].is_some());
                    }
                }
            }
        }
        Ok(TieredRun {
            value,
            decisions,
            tier_one_installs,
            tier_one_calls,
            tier_one_fallbacks,
        })
    }
}

fn call_groups(
    candidates: &[Option<TierOneFunction>],
    individual_bytes: &[usize],
) -> (Vec<bool>, Vec<usize>, Vec<Option<Vec<usize>>>) {
    let mut inbound = vec![false; candidates.len()];
    for candidate in candidates.iter().flatten() {
        for target in candidate.direct_calls() {
            if target < candidates.len() && candidates[target].is_some() {
                inbound[target] = true;
            }
        }
    }
    let mut supported = vec![false; candidates.len()];
    let mut estimates = vec![0; candidates.len()];
    let mut groups = vec![None; candidates.len()];
    let mut covered = vec![false; candidates.len()];
    for root in 0..candidates.len() {
        if candidates[root].is_none() || inbound[root] {
            continue;
        }
        let mut members = Vec::new();
        let mut visiting = vec![false; candidates.len()];
        let mut seen = vec![false; candidates.len()];
        if collect_call_group(root, candidates, &mut visiting, &mut seen, &mut members) {
            let bytes = members.iter().fold(0usize, |total, &member| {
                total.saturating_add(individual_bytes[member])
            });
            supported[root] = true;
            estimates[root] = bytes;
            for &member in &members {
                covered[member] = true;
            }
            groups[root] = Some(members);
        }
    }
    // A recursive strongly connected component has no zero-inbound member.
    // Give its lowest discovered member a group root so recursion has the
    // same atomic admission and budget treatment as an acyclic call graph.
    for root in 0..candidates.len() {
        if candidates[root].is_none() || covered[root] {
            continue;
        }
        let mut members = Vec::new();
        let mut visiting = vec![false; candidates.len()];
        let mut seen = vec![false; candidates.len()];
        if collect_call_group(root, candidates, &mut visiting, &mut seen, &mut members) {
            let canonical_root = *members.iter().min().expect("nonempty call group");
            let bytes = members.iter().fold(0usize, |total, &member| {
                total.saturating_add(individual_bytes[member])
            });
            supported[canonical_root] = true;
            estimates[canonical_root] = bytes;
            for &member in &members {
                covered[member] = true;
            }
            groups[canonical_root] = Some(members);
        }
    }
    (supported, estimates, groups)
}

fn collect_call_group(
    index: usize,
    candidates: &[Option<TierOneFunction>],
    visiting: &mut [bool],
    seen: &mut [bool],
    members: &mut Vec<usize>,
) -> bool {
    if visiting[index] {
        return true;
    }
    if seen[index] {
        return true;
    }
    let Some(candidate) = candidates[index].as_ref() else {
        return false;
    };
    visiting[index] = true;
    for target in candidate.direct_calls() {
        if target >= candidates.len()
            || !collect_call_group(target, candidates, visiting, seen, members)
        {
            return false;
        }
    }
    visiting[index] = false;
    seen[index] = true;
    members.push(index);
    true
}
