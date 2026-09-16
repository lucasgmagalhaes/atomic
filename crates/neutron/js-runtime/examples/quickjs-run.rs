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
        "hot_loop" => HOT_LOOP,
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

const HOT_LOOP: &str = r#"
(function () { let gold=1.0; let gems=1.0; let energy=1.0; let goldMult=1.0001; let gemsMult=1.00005; let energyMult=1.00002; let ticks=200000; for (let i=0; i<ticks; i++) { gold=gold*goldMult+Math.sqrt(i%997+1); gems=gems*gemsMult+(gold%13); energy=energy*energyMult+Math.log(gems+1); if (gold>1e12) { gold=gold/1e6; goldMult*=1.0000001; } if (gems>1e12) { gems=gems/1e6; gemsMult*=1.0000001; } if (energy>1e12) { energy=energy/1e6; energyMult*=1.0000001; } } return gold+gems+energy; })();
"#;
