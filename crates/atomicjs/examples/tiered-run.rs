//! @spec atomicjs-profiling#tier-one-policy
//! Reproducible Tier 1 admission/RSS probe. The interpreter remains the
//! executor, so this reports feedback and policy overhead without claiming a
//! generated-code speedup.

use atomicjs::tiering::{TierDecision, TieringPolicy};
use atomicjs::TieredProgram;

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "hot_loop".to_string());
    let repetitions = std::env::args()
        .nth(2)
        .map(|value| value.parse().expect("repetitions must be an integer"))
        .unwrap_or(100);
    let mut program = TieredProgram::compile(
        script_for(&name),
        TieringPolicy {
            enabled: true,
            call_threshold: 1,
            loop_threshold: 1,
            ..TieringPolicy::default()
        },
    )
    .unwrap();

    let t0 = std::time::Instant::now();
    let mut result = None;
    let mut eligible_functions = 0;
    for _ in 0..repetitions {
        let run = program.run().unwrap();
        eligible_functions = run
            .decisions
            .iter()
            .filter(|decision| **decision == TierDecision::Eligible)
            .count();
        result = Some(run.value);
    }
    let elapsed = t0.elapsed();

    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    println!(
        "elapsed_us={} maxrss_bytes={} repetitions={} eligible_functions={} result={:?}",
        elapsed.as_micros(),
        usage.ru_maxrss,
        repetitions,
        eligible_functions,
        result
    );
}

fn script_for(name: &str) -> &'static str {
    match name {
        "sum" => SUM,
        "hot_loop" => HOT_LOOP,
        "closures" => CLOSURES,
        other => panic!("unknown script name: {other}"),
    }
}

const HOT_LOOP: &str = include_str!("../benchmarks/scripts/hot_loop.js");
const CLOSURES: &str = include_str!("../benchmarks/scripts/closures.js");
const SUM: &str = include_str!("../benchmarks/scripts/sum.js");
