//! Comparison binary (§7.1 of spec/proposals/ATOMIC_JS_SPIKE.md): runs one
//! named reference program through `atomicjs::run_source` and prints
//! elapsed time, peak RSS, and the completion value in a shape identical to
//! `crates/js-runtime/examples/quickjs-run.rs`, so the only variable between
//! the two printed lines is the engine.

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "sum".to_string());
    let source = script_for(&name);
    let repetitions = std::env::args()
        .nth(2)
        .map(|value| value.parse().expect("repetitions must be an integer"))
        .unwrap_or(1);

    let t0 = std::time::Instant::now();
    let mut result = Ok(atomicjs::Value::Undefined);
    for _ in 0..repetitions {
        result = atomicjs::run_source(source);
    }
    let elapsed = t0.elapsed();

    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };

    println!(
        "elapsed_us={} maxrss_bytes={} repetitions={} result={:?}",
        elapsed.as_micros(), usage.ru_maxrss, repetitions, result
    );
}

fn script_for(name: &str) -> &'static str {
    match name {
        "sum" => SUM,
        "obj" => OBJ,
        "closure" => CLOSURE,
        "props" => PROPS,
        "closures" => CLOSURES,
        "hot_loop" => HOT_LOOP,
        "empty" => "",
        other => panic!("unknown script name: {other}"),
    }
}

const SUM: &str = include_str!("../benchmarks/scripts/sum.js");
const OBJ: &str = include_str!("../benchmarks/scripts/obj.js");
const CLOSURE: &str = include_str!("../benchmarks/scripts/closure.js");
const PROPS: &str = include_str!("../benchmarks/scripts/props.js");
const CLOSURES: &str = include_str!("../benchmarks/scripts/closures.js");
const HOT_LOOP: &str = include_str!("../benchmarks/scripts/hot_loop.js");
