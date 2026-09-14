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
