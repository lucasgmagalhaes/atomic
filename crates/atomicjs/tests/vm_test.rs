use atomicjs::value::Value;
use atomicjs::{run_source, run_source_with_feedback, CompiledProgram};

mod common;

fn number(source: &str) -> f64 {
    match run_source(source).unwrap_or_else(|e| panic!("run_source failed for {source:?}: {e}")) {
        Value::Number(n) => n,
        other => panic!("expected a Number completion value, got {other:?}"),
    }
}

/// The correctness gate spec/proposals/ATOMIC_JS_SPIKE.md §8 requires before
/// trusting any performance number: every reference program's exact
/// expected completion value, from §4.
#[test]
fn sum_returns_the_exact_expected_total() {
    assert_eq!(number(common::SUM), 499999500000.0);
}

#[test]
fn obj_returns_the_exact_expected_sum() {
    assert_eq!(number(common::OBJ), 30.0);
}

#[test]
fn closure_returns_the_exact_expected_third_call_value() {
    assert_eq!(number(common::CLOSURE), 3.0);
}

#[test]
fn props_returns_the_exact_expected_total() {
    assert_eq!(number(common::PROPS), 20000000.0);
}

#[test]
fn closures_returns_the_exact_expected_total() {
    assert_eq!(number(common::CLOSURES), 500000500000.0);
}

#[test]
fn each_call_to_the_same_closure_advances_independently() {
    // Regression coverage for the specific bug class closures invite: each
    // `counter()` call must see the *previous* call's incremented value,
    // not a fresh `count` or a stale one.
    let source = r#"
        function makeCounter() {
            let count = 0;
            return function () {
                return ++count;
            };
        }
        const counter = makeCounter();
        counter();
        counter();
        counter();
    "#;
    assert_eq!(number(source), 3.0);
}

#[test]
fn two_independent_counters_do_not_share_captured_state() {
    // A different closure-specific bug class: two calls to `makeCounter`
    // must each get their *own* `count` cell, not one shared between them.
    let source = r#"
        function makeCounter() {
            let count = 0;
            return function () {
                return ++count;
            };
        }
        const a = makeCounter();
        const b = makeCounter();
        a();
        a();
        b();
        a();
    "#;
    assert_eq!(number(source), 3.0);
}

#[test]
fn named_functions_can_call_another_named_function() {
    let source = r#"
        function add(a, b) { return a + b; }
        function doubleSum(a, b) { return add(a, b) * 2; }
        doubleSum(3, 4);
    "#;
    assert_eq!(number(source), 14.0);
}

#[test]
fn captured_object_property_uses_the_same_member_semantics() {
    let source = r#"
        function makeReader() {
            const player = { damage: 20 };
            return function () { return player.damage; };
        }
        const reader = makeReader();
        reader();
    "#;
    assert_eq!(number(source), 20.0);
}

#[test]
fn later_literal_property_overwrites_the_earlier_value() {
    assert_eq!(
        number("const player = { level: 1, level: 2 }; player.level;"),
        2.0
    );
}

#[test]
fn property_cache_reloads_the_slot_when_an_object_layout_changes() {
    // Both reads execute at the same bytecode site. The second object puts
    // `damage` in a different slot, so a stale cache would incorrectly read
    // `bonus` (1) instead of damage (7).
    let source = r#"
        let player = { damage: 20 };
        let total = 0;
        for (let i = 0; i < 2; i++) {
            if (i > 0) { player = { bonus: 1, damage: 7 }; }
            total += player.damage;
        }
        total;
    "#;
    assert_eq!(number(source), 27.0);
}

#[test]
fn direct_call_accumulation_reads_the_left_value_before_the_call() {
    let source = r#"
        let total = 1;
        function next() { total = 2; return 3; }
        total += next();
        total;
    "#;
    // JavaScript evaluates the left-hand value before calling `next`, so
    // this is 1 + 3 rather than the post-call value 2 + 3.
    assert_eq!(number(source), 4.0);
}

#[test]
fn recursive_upgrade_cost_matches_quickjs_golden_value() {
    assert_eq!(number(common::UPGRADE_COST), 19.0);
}

#[test]
fn empty_script_completes_to_undefined() {
    assert!(matches!(run_source("").unwrap(), Value::Undefined));
}

#[test]
fn compiled_program_reuses_bytecode_but_not_execution_state() {
    let program = CompiledProgram::compile(common::CLOSURE).unwrap();
    assert_eq!(
        match program.run().unwrap() {
            Value::Number(n) => n,
            other => panic!("expected Number, got {other:?}"),
        },
        3.0
    );
    assert_eq!(
        match program.run().unwrap() {
            Value::Number(n) => n,
            other => panic!("expected Number, got {other:?}"),
        },
        3.0
    );
}

#[test]
fn compiled_program_keeps_feedback_opt_in_per_run() {
    let program = CompiledProgram::compile(common::SUM).unwrap();
    let (value, feedback) = program.run_with_feedback().unwrap();
    assert!(matches!(value, Value::Number(n) if n == 499999500000.0));
    assert_eq!(feedback[0].loop_count, 1_000_000);
}

#[test]
fn a_non_expression_last_statement_completes_to_undefined() {
    let source = "let x = 1;";
    assert!(matches!(run_source(source).unwrap(), Value::Undefined));
}

#[test]
fn missing_property_read_is_undefined_not_an_error() {
    let source = "const player = { level: 10 }; player.doesNotExist;";
    assert!(matches!(run_source(source).unwrap(), Value::Undefined));
}

#[test]
fn calling_a_non_function_is_a_runtime_error_not_a_panic() {
    let source = "const player = { level: 10 }; player.level();";
    assert!(run_source(source).is_err());
}

#[test]
fn feedback_call_count_reflects_the_real_number_of_calls() {
    let (_value, feedback) = run_source_with_feedback(common::CLOSURE).unwrap();
    // function[0] = inner closure (called 3 times), function[1] =
    // makeCounter (called once), function[2] = top level (called once, to
    // start execution) - matches compiler_test.rs's own index reasoning for
    // this exact program.
    assert_eq!(
        feedback[0].call_count, 3,
        "inner closure should be called 3 times"
    );
    assert_eq!(
        feedback[1].call_count, 1,
        "makeCounter should be called once"
    );
    assert_eq!(feedback[2].call_count, 1, "the top level runs exactly once");
}

#[test]
fn feedback_loop_count_reflects_the_real_number_of_iterations() {
    let (_value, feedback) = run_source_with_feedback(common::SUM).unwrap();
    // function[0] = sum, function[1] = top level (see compiler_test.rs).
    // sum(1000000) loops from i=0 to i=999999 - 1,000,000 back-edges.
    assert_eq!(feedback[0].loop_count, 1_000_000);
}

#[test]
fn every_reference_program_runs_to_completion_without_error() {
    for (name, source) in common::all() {
        run_source(source).unwrap_or_else(|e| panic!("run_source failed for {name:?}: {e}"));
    }
}

#[test]
fn feedback_is_zero_for_a_function_that_is_never_called() {
    let source = r#"
        function neverCalled() {
            return 1;
        }
        1;
    "#;
    let (_value, feedback) = run_source_with_feedback(source).unwrap();
    assert_eq!(feedback[0].call_count, 0);
}

#[test]
fn executes_hot_loop_language_subset_and_native_math() {
    let source = r#"
        let x = 5e0;
        x = x * 2 / 2 % 7;
        if (x > 3) { x *= Math.sqrt(4) + Math.log(1); }
        x;
    "#;
    assert_eq!(number(source), 10.0);
}

#[test]
fn hot_loop_matches_quickjs_golden_value_exactly() {
    assert_eq!(number(common::HOT_LOOP), 3_022_237_872.307_638_6);
}
