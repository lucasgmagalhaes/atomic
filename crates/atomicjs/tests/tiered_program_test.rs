//! @spec atomicjs-profiling#tier-one-lifecycle
use atomicjs::tiering::{HostState, TierDecision, TieringPolicy};
use atomicjs::{TieredProgram, Value};

const SOURCE: &str = r#"
function increment(n) { return n + 1; }
increment(41);
"#;

fn eager_policy() -> TieringPolicy {
    TieringPolicy {
        enabled: true,
        call_threshold: 1,
        loop_threshold: u32::MAX,
        code_budget_bytes: 64 * 1024,
    }
}

fn assert_number(value: Value, expected: f64) {
    match value {
        Value::Number(actual) => assert_eq!(actual, expected),
        other => panic!("expected Number({expected}), got {other:?}"),
    }
}

#[test]
fn tiered_program_keeps_interpreter_result_while_admitting_hot_functions() {
    let mut program = TieredProgram::compile(SOURCE, eager_policy()).unwrap();

    let first = program.run().unwrap();
    assert_number(first.value, 42.0);
    assert!(first.decisions.contains(&TierDecision::Eligible));

    let second = program.run().unwrap();
    assert_number(second.value, 42.0);
    assert_eq!(second.decisions, first.decisions);
    assert_eq!(second.tier_one_calls, 1);
}

#[test]
fn paused_tiered_program_uses_tier_zero_and_forgets_admission() {
    let mut program = TieredProgram::compile(SOURCE, eager_policy()).unwrap();
    assert!(program
        .run()
        .unwrap()
        .decisions
        .contains(&TierDecision::Eligible));

    program.set_host_state(HostState::Paused);
    let paused = program.run().unwrap();
    assert_number(paused.value, 42.0);
    assert!(paused
        .decisions
        .iter()
        .all(|decision| *decision == TierDecision::Interpret));
    assert_eq!(paused.tier_one_calls, 0);
}

#[test]
fn resume_after_pause_requires_a_fresh_tier_one_warmup() {
    let mut program = TieredProgram::compile(SOURCE, eager_policy()).unwrap();
    program.run().unwrap();

    program.set_host_state(HostState::Paused);
    program.set_host_state(HostState::Running);
    let after_resume = program.run().unwrap();
    assert_eq!(after_resume.tier_one_calls, 0);
    assert_eq!(after_resume.tier_one_installs, 1);

    let accelerated = program.run().unwrap();
    assert_eq!(accelerated.tier_one_calls, 1);
}
