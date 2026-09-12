//! Synthetic idle-game-shaped hot loop: per-tick resource accumulation
//! across several currencies with multiplier upgrades, run for many
//! iterations. Not a real game script — a stand-in for "what a heavy
//! idle-game tick handler does": repeated floating-point arithmetic over
//! a handful of numeric fields, no DOM/allocation churn. Answers whether
//! QuickJS (interpreter, no JIT) is fast enough for this shape of load
//! before considering a JIT'd engine swap (see plan Track A).

use criterion::{criterion_group, criterion_main, Criterion};
use js_runtime::{Context, Runtime};

const HOT_LOOP_JS: &str = r#"
(function () {
    let gold = 1.0;
    let gems = 1.0;
    let energy = 1.0;
    let goldMult = 1.0001;
    let gemsMult = 1.00005;
    let energyMult = 1.00002;
    let ticks = 200000;
    for (let i = 0; i < ticks; i++) {
        gold = gold * goldMult + Math.sqrt(i % 997 + 1);
        gems = gems * gemsMult + (gold % 13);
        energy = energy * energyMult + Math.log(gems + 1);
        if (gold > 1e12) { gold = gold / 1e6; goldMult *= 1.0000001; }
        if (gems > 1e12) { gems = gems / 1e6; gemsMult *= 1.0000001; }
        if (energy > 1e12) { energy = energy / 1e6; energyMult *= 1.0000001; }
    }
    return gold + gems + energy;
})()
"#;

fn bench_hot_loop(c: &mut Criterion) {
    c.bench_function("quickjs_idle_hot_loop_200k_ticks", |b| {
        b.iter(|| {
            let runtime = Runtime::new();
            let ctx = Context::new(&runtime);
            ctx.eval(HOT_LOOP_JS, "hot_loop.js")
                .expect("hot loop script must not throw");
        });
    });
}

criterion_group!(benches, bench_hot_loop);
criterion_main!(benches);
