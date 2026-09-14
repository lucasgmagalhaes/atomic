//! Comparison binary (§7.1 of spec/proposals/ATOMIC_JS_SPIKE.md): runs one
//! named reference program through the real QuickJS-ng engine already
//! embedded in this crate, and prints elapsed time, peak RSS, and the
//! completion value in a shape identical to
//! `crates/atomicjs/examples/atomicjs-run.rs`, so the only variable between
//! the two printed lines is the engine.

use js_runtime::{Context, Runtime};

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "sum".to_string());
    let source = script_for(&name);

    let t0 = std::time::Instant::now();
    let runtime = Runtime::new();
    let ctx = Context::new(&runtime);
    let result = ctx.eval(source, "quickjs-run.js");
    let elapsed = t0.elapsed();

    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };

    println!(
        "elapsed_us={} maxrss_bytes={} result={:?}",
        elapsed.as_micros(),
        usage.ru_maxrss,
        result
    );
}

fn script_for(name: &str) -> &'static str {
    match name {
        "sum" => SUM,
        "obj" => OBJ,
        "closure" => CLOSURE,
        "props" => PROPS,
        "closures" => CLOSURES,
        "empty" => "",
        other => panic!("unknown script name: {other}"),
    }
}

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

const OBJ: &str = r#"
const player = { level: 10, damage: 20 };
player.damage + player.level;
"#;

const CLOSURE: &str = r#"
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
