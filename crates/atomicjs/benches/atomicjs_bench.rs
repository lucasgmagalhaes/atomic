//! In-process regression benchmarks for AtomicJS.
//!
//! Each reference workload has two measurements: `compile_and_run` keeps the
//! public one-shot API honest, while `run_compiled` measures the host-ready
//! compile-once/run-many boundary without leaking execution state between runs.

use atomicjs::{run_source, CompiledProgram};
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

const SUM: &str = include_str!("../benchmarks/scripts/sum.js");
const PROPS: &str = include_str!("../benchmarks/scripts/props.js");
const CLOSURES: &str = include_str!("../benchmarks/scripts/closures.js");
const HOT_LOOP: &str = include_str!("../benchmarks/scripts/hot_loop.js");

fn bench_reference_workloads(criterion: &mut Criterion) {
    for (name, source) in [
        ("sum", SUM),
        ("props", PROPS),
        ("closures", CLOSURES),
        ("hot_loop", HOT_LOOP),
    ] {
        criterion.bench_function(&format!("{name}/compile_and_run"), |bench| {
            bench.iter(|| black_box(run_source(black_box(source)).unwrap()))
        });

        let program = CompiledProgram::compile(source).unwrap();
        criterion.bench_function(&format!("{name}/run_compiled"), |bench| {
            bench.iter(|| black_box(program.run().unwrap()))
        });
    }
}

criterion_group!(benches, bench_reference_workloads);
criterion_main!(benches);
