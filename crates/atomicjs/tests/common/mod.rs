//! Shared fixtures for atomicjs's integration tests: the five named
//! reference programs from spec/proposals/ATOMIC_JS_SPIKE.md §4. Kept in one
//! place for the tests directory only (each `tests/*_test.rs` file is its
//! own compiled crate, so this common module avoids duplicating five scripts
//! across every test file that needs them) — deliberately not shared with
//! `examples/atomicjs-run.rs`/`crates/js-runtime/examples/quickjs-run.rs`,
//! which duplicate them inline per §4.3's "no shared file" convention for
//! that specific case (different crates, different working directories).

pub const SUM: &str = r#"
function sum(n) {
    let total = 0;
    for (let i = 0; i < n; i++) {
        total += i;
    }
    return total;
}
sum(1000000);
"#;

pub const OBJ: &str = r#"
const player = { level: 10, damage: 20 };
player.damage + player.level;
"#;

pub const CLOSURE: &str = r#"
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

pub const PROPS: &str = r#"
const player = { level: 10, damage: 20 };
let total = 0;
for (let i = 0; i < 1000000; i++) {
    total += player.damage;
}
total;
"#;

pub const CLOSURES: &str = r#"
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

/// The bounded representative workload from `ATOMIC_JS_MILESTONE_2.md`.
/// The comparison binaries intentionally keep their own inline copies; this
/// fixture exists solely for the AtomicJS correctness gate.
#[allow(dead_code)]
pub const HOT_LOOP: &str = r#"
(function () { let gold=1.0; let gems=1.0; let energy=1.0; let goldMult=1.0001; let gemsMult=1.00005; let energyMult=1.00002; let ticks=200000; for (let i=0; i<ticks; i++) { gold=gold*goldMult+Math.sqrt(i%997+1); gems=gems*gemsMult+(gold%13); energy=energy*energyMult+Math.log(gems+1); if (gold>1e12) { gold=gold/1e6; goldMult*=1.0000001; } if (gems>1e12) { gems=gems/1e6; gemsMult*=1.0000001; } if (energy>1e12) { energy=energy/1e6; energyMult*=1.0000001; } } return gold+gems+energy; })();
"#;

/// Milestone 3's recursion and independent named-function call graph.
#[allow(dead_code)]
pub const UPGRADE_COST: &str = r#"
function cost(level) {
    if (level < 1) { return 10; }
    return cost(level - 1) * 1.15;
}
function affordableLevels(budget, level) {
    let price = cost(level);
    if (price > budget) { return level; }
    return affordableLevels(budget - price, level + 1);
}
affordableLevels(1000, 0);
"#;

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
