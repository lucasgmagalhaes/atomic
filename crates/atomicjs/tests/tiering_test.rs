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
    assert_eq!(policy.decide(HostState::Running, &hot, 4, 6), TierDecision::Eligible);
    assert_eq!(policy.decide(HostState::Paused, &hot, 0, 1), TierDecision::Interpret);
    assert_eq!(policy.decide(HostState::Running, &hot, 5, 6), TierDecision::Interpret);
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
    assert_eq!(controller.observe(&run, &[1]), vec![TierDecision::Interpret]);
    assert_eq!(controller.observe(&run, &[1]), vec![TierDecision::Eligible]);
    controller.set_host_state(HostState::Paused);
    assert_eq!(controller.observe(&run, &[1]), vec![TierDecision::Interpret]);
}
