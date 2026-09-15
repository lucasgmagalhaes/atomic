//! @spec atomicjs-profiling#tier-one-lifecycle
use atomicjs::tiering::{HostState, TierDecision, TieringController, TieringPolicy};
use atomicjs::FunctionFeedback;

#[test]
fn only_running_hot_functions_within_budget_are_eligible() {
    let policy = TieringPolicy {
        enabled: true,
        call_threshold: 3,
        loop_threshold: 9,
        code_budget_bytes: 10,
    };
    let hot = FunctionFeedback {
        call_count: 3,
        loop_count: 0,
    };
    assert_eq!(
        policy.decide(HostState::Running, &hot, 4, 6),
        TierDecision::Eligible
    );
    assert_eq!(
        policy.decide(HostState::Paused, &hot, 0, 1),
        TierDecision::Interpret
    );
    assert_eq!(
        policy.decide(HostState::Running, &hot, 5, 6),
        TierDecision::Interpret
    );
}

#[test]
fn controller_accumulates_runs_and_pause_restores_tier_zero() {
    let mut controller = TieringController::new(
        TieringPolicy {
            enabled: true,
            call_threshold: 2,
            loop_threshold: 9,
            code_budget_bytes: 10,
        },
        1,
    );
    let run = [FunctionFeedback {
        call_count: 1,
        loop_count: 0,
    }];
    assert_eq!(
        controller.observe(&run, &[1], &[true]),
        vec![TierDecision::Interpret]
    );
    assert_eq!(
        controller.observe(&run, &[1], &[true]),
        vec![TierDecision::Eligible]
    );
    controller.set_host_state(HostState::Paused);
    assert_eq!(
        controller.observe(&run, &[1], &[true]),
        vec![TierDecision::Interpret]
    );
}

#[test]
fn controller_reserves_budget_for_the_first_admitted_function() {
    let mut controller = TieringController::new(
        TieringPolicy {
            enabled: true,
            call_threshold: 1,
            loop_threshold: 1,
            code_budget_bytes: 10,
        },
        2,
    );
    let first_hot = FunctionFeedback {
        call_count: 1,
        loop_count: 0,
    };
    let second_hot = FunctionFeedback {
        call_count: 1,
        loop_count: 0,
    };

    assert_eq!(
        controller.observe(&[first_hot, second_hot], &[6, 6], &[true, true]),
        vec![TierDecision::Eligible, TierDecision::Interpret]
    );
}

#[test]
fn unsupported_functions_never_reserve_budget_or_become_eligible() {
    let mut controller = TieringController::new(
        TieringPolicy {
            enabled: true,
            call_threshold: 1,
            loop_threshold: 1,
            code_budget_bytes: 1,
        },
        2,
    );
    assert_eq!(
        controller.observe(
            &[
                FunctionFeedback {
                    call_count: 1,
                    loop_count: 0,
                },
                FunctionFeedback {
                    call_count: 1,
                    loop_count: 0,
                },
            ],
            &[0, 1],
            &[false, true],
        ),
        vec![TierDecision::Interpret, TierDecision::Eligible]
    );
}

#[test]
fn pause_clears_feedback_and_requires_fresh_warmup_after_resume() {
    let mut controller = TieringController::new(
        TieringPolicy {
            enabled: true,
            call_threshold: 2,
            loop_threshold: u32::MAX,
            code_budget_bytes: 10,
        },
        1,
    );
    let run = [FunctionFeedback {
        call_count: 1,
        loop_count: 0,
    }];

    assert_eq!(
        controller.observe(&run, &[1], &[true]),
        vec![TierDecision::Interpret]
    );
    controller.set_host_state(HostState::Paused);
    controller.set_host_state(HostState::Running);
    assert_eq!(
        controller.observe(&run, &[1], &[true]),
        vec![TierDecision::Interpret]
    );
    assert_eq!(
        controller.observe(&run, &[1], &[true]),
        vec![TierDecision::Eligible]
    );
}
