//! Real CSS cascade benchmarks — `spec/architecture/performance.md`
//! §21.3's suite. `matching_declarations` is called once per element
//! during box-tree construction (see `layout_engine::tree::build`), so
//! "N nodes / M rules" here means N calls against an M-rule sheet, the
//! realistic per-element hot-path shape rather than a single whole-tree
//! call (this crate has no such whole-tree entry point — that's
//! `layout-engine`'s job).
use criterion::{criterion_group, criterion_main, Criterion};
use css::{
    build_selector_index, matching_declarations, matching_declarations_indexed, parse_stylesheet,
    ElementSnapshot,
};

fn el<'a>(tag: &'a str, classes: Vec<&'a str>) -> ElementSnapshot<'a> {
    ElementSnapshot {
        tag,
        classes,
        ..Default::default()
    }
}

fn flat_sheet(rule_count: usize) -> css::Stylesheet {
    let mut src = String::new();
    for i in 0..rule_count {
        src.push_str(&format!(".class-{i} {{ color: red; width: {i}px; }}\n"));
    }
    parse_stylesheet(&src)
}

fn bench_nodes_vs_rules(c: &mut Criterion) {
    let mut group = c.benchmark_group("matching_declarations_nodes_vs_rules");
    for (nodes, rules) in [(1_000, 100), (10_000, 500)] {
        let sheet = flat_sheet(rules);
        let chain = [el("div", vec!["class-0", "class-1"])];
        group.bench_function(format!("{nodes}_nodes_{rules}_rules"), |b| {
            b.iter(|| {
                for _ in 0..nodes {
                    let _ = matching_declarations(&sheet, &chain, 1024.0, 768.0);
                }
            });
        });
    }
    group.finish();
}

/// Same shape as `bench_nodes_vs_rules`, but comparing `SelectorIndex`'s
/// indexed path against the unindexed one - `flat_sheet` gives every
/// rule a distinct class name, so a target matching only 2 of them via
/// `matching_declarations_indexed` should check far fewer candidate
/// selectors than `matching_declarations`'s "every rule" scan once the
/// rule count is large. The index itself is built once outside the
/// timed loop, matching its real call shape (`layout_engine::tree`
/// builds it once per `build_box_tree` call, not once per element).
fn bench_indexed_vs_unindexed(c: &mut Criterion) {
    let mut group = c.benchmark_group("matching_declarations_indexed_vs_unindexed");
    for (nodes, rules) in [(1_000, 100), (10_000, 500)] {
        let sheet = flat_sheet(rules);
        let index = build_selector_index(&sheet);
        let chain = [el("div", vec!["class-0", "class-1"])];
        group.bench_function(format!("unindexed_{nodes}_nodes_{rules}_rules"), |b| {
            b.iter(|| {
                for _ in 0..nodes {
                    let _ = matching_declarations(&sheet, &chain, 1024.0, 768.0);
                }
            });
        });
        group.bench_function(format!("indexed_{nodes}_nodes_{rules}_rules"), |b| {
            b.iter(|| {
                for _ in 0..nodes {
                    let _ = matching_declarations_indexed(&index, &sheet, &chain, 1024.0, 768.0);
                }
            });
        });
    }
    group.finish();
}

fn bench_deep_selectors(c: &mut Criterion) {
    // A real deep descendant/compound chain (`div.a div.b div.c ... span`)
    // forces `selector_matches` to walk every `preceding_siblings`/ancestor
    // link in the snapshot rather than matching on the first compound.
    let sheet = parse_stylesheet(
        "div.a div.b div.c div.d div.e div.f div.g div.h span.target { color: blue; }",
    );
    // Nested via `preceding_siblings` isn't ancestry - this crate's
    // `ElementSnapshot` doesn't model a parent chain directly (descendant
    // combinators walk something else internally); a single flat snapshot
    // with the target's own tag/class is the honest shape this crate's
    // public API can build without access to a real `dom::Dom` tree, so
    // this measures the sheet-side cost (8 compounds to reject/accept
    // per rule check) rather than a full ancestor walk.
    let chain = [el("span", vec!["target"])];
    c.bench_function("deep_selector_8_compounds_1000_calls", |b| {
        b.iter(|| {
            for _ in 0..1_000 {
                let _ = matching_declarations(&sheet, &chain, 1024.0, 768.0);
            }
        });
    });
}

fn bench_parse_stylesheet(c: &mut Criterion) {
    let src_500 = {
        let mut s = String::new();
        for i in 0..500 {
            s.push_str(&format!(".class-{i} {{ color: red; width: {i}px; }}\n"));
        }
        s
    };
    c.bench_function("parse_stylesheet_500_rules", |b| {
        b.iter(|| parse_stylesheet(&src_500));
    });
}

criterion_group!(
    benches,
    bench_nodes_vs_rules,
    bench_indexed_vs_unindexed,
    bench_deep_selectors,
    bench_parse_stylesheet
);
criterion_main!(benches);
