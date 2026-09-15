//! In-process regression benchmarks for AtomicJS.
//!
//! Each reference workload has two measurements: `compile_and_run` keeps the
//! public one-shot API honest, while `run_compiled` measures the host-ready
//! compile-once/run-many boundary without leaking execution state between runs.

// @spec atomicjs-profiling#tier-one-policy
use atomicjs::tiering::TieringPolicy;
use atomicjs::{run_source, CompiledProgram, TieredProgram};
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

const SUM: &str = include_str!("../benchmarks/scripts/sum.js");
const PROPS: &str = include_str!("../benchmarks/scripts/props.js");
const CLOSURES: &str = include_str!("../benchmarks/scripts/closures.js");
const HOT_LOOP: &str = include_str!("../benchmarks/scripts/hot_loop.js");
const LOCAL_CONST: &str = include_str!("../benchmarks/scripts/local_const.js");
const NUMERIC_CALL_GRAPH: &str = r#"
function increment(n) { return n + 1; }
function accumulate(n) {
    let total = 0;
    for (let i = 0; i < n; i++) { total += increment(i); }
    return total;
}
accumulate(1000000);
"#;
const PROPERTY_LOOP: &str = r#"
function sum(player, n) {
    let total = 0;
    for (let i = 0; i < n; i++) { total += player.damage; }
    return total;
}
const player = { damage: 20 };
sum(player, 1000000);
"#;

fn bench_reference_workloads(criterion: &mut Criterion) {
    for (name, source) in [
        ("sum", SUM),
        ("props", PROPS),
        ("closures", CLOSURES),
        ("hot_loop", HOT_LOOP),
        ("local_const", LOCAL_CONST),
    ] {
        criterion.bench_function(&format!("{name}/compile_and_run"), |bench| {
            bench.iter(|| black_box(run_source(black_box(source)).unwrap()))
        });

        let program = CompiledProgram::compile(source).unwrap();
        criterion.bench_function(&format!("{name}/run_compiled"), |bench| {
            bench.iter(|| black_box(program.run().unwrap()))
        });
    }

    // `sum` is in the Tier 1 numeric subset. Warm it once to admit/compile
    // the function, then measure steady-state specialized execution.
    let mut tiered_sum = TieredProgram::compile(
        SUM,
        TieringPolicy {
            enabled: true,
            call_threshold: 1,
            loop_threshold: 1,
            ..TieringPolicy::default()
        },
    )
    .unwrap();
    tiered_sum.run().unwrap();
    criterion.bench_function("sum/tier_one_compiled", |bench| {
        bench.iter(|| black_box(tiered_sum.run().unwrap()))
    });

    // A hot root and its numeric helper are admitted as one Tier-1 graph.
    let mut tiered_call_graph = TieredProgram::compile(
        NUMERIC_CALL_GRAPH,
        TieringPolicy {
            enabled: true,
            call_threshold: 1,
            loop_threshold: 1,
            ..TieringPolicy::default()
        },
    )
    .unwrap();
    tiered_call_graph.run().unwrap();
    criterion.bench_function("numeric_call_graph/tier_one_compiled", |bench| {
        bench.iter(|| black_box(tiered_call_graph.run().unwrap()))
    });

    let property_loop = CompiledProgram::compile(PROPERTY_LOOP).unwrap();
    criterion.bench_function("property_loop/tier_zero_compiled", |bench| {
        bench.iter(|| black_box(property_loop.run().unwrap()))
    });

    let mut tiered_property_loop = TieredProgram::compile(
        PROPERTY_LOOP,
        TieringPolicy {
            enabled: true,
            call_threshold: 1,
            loop_threshold: 1,
            ..TieringPolicy::default()
        },
    )
    .unwrap();
    tiered_property_loop.run().unwrap();
    criterion.bench_function("property_loop/tier_one_compiled", |bench| {
        bench.iter(|| black_box(tiered_property_loop.run().unwrap()))
    });
}

criterion_group!(benches, bench_reference_workloads);
criterion_main!(benches);
