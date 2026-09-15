//! Differential correctness gate for the intentionally supported AtomicJS
//! subset. QuickJS-ng is a development-only oracle: no production crate
//! depends on AtomicJS, and this test is never part of its public runtime API.

use atomicjs::run_source;
use js_runtime::{Context, Runtime};

fn quickjs_result(source: &str) -> String {
    let runtime = Runtime::new();
    let context = Context::new(&runtime);
    context.eval(source, "atomicjs-differential.js").unwrap()
}

fn assert_matches_quickjs(name: &str, source: &str) {
    let atomic = run_source(source)
        .unwrap_or_else(|error| panic!("AtomicJS failed {name}: {error}"))
        .to_string();
    let quickjs = quickjs_result(source);
    assert_eq!(atomic, quickjs, "differential mismatch in {name}: {source}");
}

#[test]
fn fixed_supported_programs_match_quickjs() {
    let cases = [
        (
            "objects_and_members",
            "const player = { level: 10, damage: 20 }; player.level + player.damage;",
        ),
        (
            "closure_state",
            r#"
                function makeCounter() { let count = 0; return function () { return ++count; }; }
                const counter = makeCounter(); counter(); counter(); counter();
            "#,
        ),
        (
            "if_and_native_math",
            "let value = Math.sqrt(81) + Math.log(1); if (value > 8) { value *= 2; } value;",
        ),
        (
            "recursive_cross_function_call",
            r#"
                function cost(level) { if (level < 1) { return 10; } return cost(level - 1) * 1.15; }
                function affordable(budget, level) { let price = cost(level); if (price > budget) { return level; } return affordable(budget - price, level + 1); }
                affordable(1000, 0);
            "#,
        ),
    ];

    for (name, source) in cases {
        assert_matches_quickjs(name, source);
    }
}

#[test]
fn generated_arithmetic_and_control_cases_match_quickjs() {
    for (index, (start, add, multiplier, divisor, modulus)) in [
        (2, 3, 5, 2, 7),
        (7, 11, 3, 5, 9),
        (19, 2, 7, 3, 11),
        (31, 13, 2, 4, 17),
        (43, 5, 11, 6, 19),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!(
            "let value = {start}; value = (value + {add}) * {multiplier}; value = value / {divisor} % {modulus}; if (value > 1) {{ value = value - 1; }} value;"
        );
        assert_matches_quickjs(&format!("generated_case_{index}"), &source);
    }
}
