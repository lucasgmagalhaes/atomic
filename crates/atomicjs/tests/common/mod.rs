//! Shared fixtures for AtomicJS's integration tests. The named reference
//! programs live in `benchmarks/scripts`, which is the crate's single source
//! of truth for the runner, tests, and in-process benchmarks.

pub const SUM: &str = include_str!("../../benchmarks/scripts/sum.js");
pub const OBJ: &str = include_str!("../../benchmarks/scripts/obj.js");
pub const CLOSURE: &str = include_str!("../../benchmarks/scripts/closure.js");
pub const PROPS: &str = include_str!("../../benchmarks/scripts/props.js");
pub const CLOSURES: &str = include_str!("../../benchmarks/scripts/closures.js");

/// The bounded representative workload from `ATOMIC_JS_MILESTONE_2.md`.
#[allow(dead_code)]
pub const HOT_LOOP: &str = include_str!("../../benchmarks/scripts/hot_loop.js");

/// Milestone 3's recursion and independent named-function call graph.
#[allow(dead_code)]
pub const UPGRADE_COST: &str = include_str!("../../benchmarks/scripts/upgrade_cost.js");

/// `(name, source)` pairs for every reference program, matching the CLI arg
/// names both comparison binaries accept (§4.3).
pub fn all() -> [(&'static str, &'static str); 5] {
    [
        ("sum", SUM),
        ("obj", OBJ),
        ("closure", CLOSURE),
        ("props", PROPS),
        ("closures", CLOSURES),
    ]
}
