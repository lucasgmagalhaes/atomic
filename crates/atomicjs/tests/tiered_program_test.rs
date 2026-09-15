//! @spec atomicjs-profiling#tier-one-inlining
use atomicjs::tiering::{HostState, TierDecision, TieringPolicy};
use atomicjs::{TieredProgram, Value};

const SOURCE: &str = r#"
function increment(n) { return n + 1; }
increment(41);
"#;

const CALL_GRAPH_SOURCE: &str = r#"
function increment(n) { return n + 1; }
function twiceIncremented(n) { return increment(increment(n)); }
twiceIncremented(40);
"#;

const CONSTANT_LEFT_HELPER_SOURCE: &str = r#"
function remaining(n) { return 100 - n; }
function invoke(n) { return remaining(n); }
invoke(42);
"#;

const BINARY_HELPER_SOURCE: &str = r#"
function remainder(left, right) { return left % right; }
function invoke(left, right) { return remainder(left, right); }
invoke(17, 5);
"#;

const RELATIONAL_HELPER_SOURCE: &str = r#"
function isBelow(left, right) { return left < right; }
function choose(left, right) {
    if (isBelow(left, right)) { return 1; }
    return 0;
}
choose(3, 7);
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

#[test]
fn tiered_program_admits_a_numeric_direct_call_graph_atomically() {
    let mut program = TieredProgram::compile(CALL_GRAPH_SOURCE, eager_policy()).unwrap();

    let first = program.run().unwrap();
    assert_number(first.value, 42.0);
    assert_eq!(
        first.tier_one_installs, 2,
        "root and helper install together"
    );

    let accelerated = program.run().unwrap();
    assert_number(accelerated.value, 42.0);
    assert_eq!(accelerated.tier_one_calls, 1);
    assert_eq!(accelerated.tier_one_fallbacks, 0);
}

#[test]
fn tiered_program_inlines_a_constant_left_numeric_helper() {
    let mut program = TieredProgram::compile(CONSTANT_LEFT_HELPER_SOURCE, eager_policy()).unwrap();

    assert_number(program.run().unwrap().value, 58.0);
    let accelerated = program.run().unwrap();
    assert_number(accelerated.value, 58.0);
    assert_eq!(accelerated.tier_one_calls, 1);
    assert_eq!(accelerated.tier_one_fallbacks, 0);
}

#[test]
fn tiered_program_inlines_a_binary_numeric_helper() {
    let mut program = TieredProgram::compile(BINARY_HELPER_SOURCE, eager_policy()).unwrap();

    assert_number(program.run().unwrap().value, 2.0);
    let accelerated = program.run().unwrap();
    assert_number(accelerated.value, 2.0);
    assert_eq!(accelerated.tier_one_calls, 1);
    assert_eq!(accelerated.tier_one_fallbacks, 0);
}

#[test]
fn tiered_program_inlines_a_relational_helper_for_a_conditional() {
    let mut program = TieredProgram::compile(RELATIONAL_HELPER_SOURCE, eager_policy()).unwrap();

    assert_number(program.run().unwrap().value, 1.0);
    let accelerated = program.run().unwrap();
    assert_number(accelerated.value, 1.0);
    assert_eq!(accelerated.tier_one_calls, 1);
    assert_eq!(accelerated.tier_one_fallbacks, 0);
}

#[test]
fn tiered_program_rejects_an_entire_graph_when_the_budget_cannot_hold_it() {
    let mut policy = eager_policy();
    policy.code_budget_bytes = 1;
    let mut program = TieredProgram::compile(CALL_GRAPH_SOURCE, policy).unwrap();

    let first = program.run().unwrap();
    assert_number(first.value, 42.0);
    assert_eq!(first.tier_one_installs, 0);

    let second = program.run().unwrap();
    assert_number(second.value, 42.0);
    assert_eq!(second.tier_one_calls, 0);
}
