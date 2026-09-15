//! In-process regression benchmarks for AtomicJS.
//!
//! Each reference workload has two measurements: `compile_and_run` keeps the
//! public one-shot API honest, while `run_compiled` measures the host-ready
//! compile-once/run-many boundary without leaking execution state between runs.

use atomicjs::{run_source, CompiledProgram};
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

const SUM: &str = r#"
function sum(n) {
    let total = 0;
    for (let i = 0; i < n; i++) {
        total += i;
    }
    return total;
}
sum(1000000);
"#;

const PROPS: &str = r#"
const player = { level: 10, damage: 20 };
let total = 0;
for (let i = 0; i < 1000000; i++) {
    total += player.damage;
}
total;
"#;

const CLOSURES: &str = r#"
function makeCounter() {
    let count = 0;
    return function () {
        return ++count;
    };
}
const counter = makeCounter();
let total = 0;
for (let i = 0; i < 1000000; i++) {
    total += counter();
}
total;
"#;

const HOT_LOOP: &str = r#"
(function () { let gold=1.0; let gems=1.0; let energy=1.0; let goldMult=1.0001; let gemsMult=1.00005; let energyMult=1.00002; let ticks=200000; for (let i=0; i<ticks; i++) { gold=gold*goldMult+Math.sqrt(i%997+1); gems=gems*gemsMult+(gold%13); energy=energy*energyMult+Math.log(gems+1); if (gold>1e12) { gold=gold/1e6; goldMult*=1.0000001; } if (gems>1e12) { gems=gems/1e6; gemsMult*=1.0000001; } if (energy>1e12) { energy=energy/1e6; energyMult*=1.0000001; } } return gold+gems+energy; })();
"#;

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
