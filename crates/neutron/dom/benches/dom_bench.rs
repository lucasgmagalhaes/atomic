//! Real `dom::Dom` operation benchmarks — the DOM half of
//! `spec/architecture/performance.md` §21.1's suite (querySelector/
//! querySelectorAll/getElementsByClassName/innerHTML/outerHTML aren't
//! modeled at this crate's level — they live in `js-runtime`'s
//! `dom_bindings`/`css` selector matching, or aren't implemented at
//! all — so this covers the real subset `dom::Dom` itself exposes:
//! node creation, appending, subtree removal, subtree cloning, and
//! `#id` lookup/attribute access).
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use dom::Dom;

fn bench_create_nodes(c: &mut Criterion) {
    let mut group = c.benchmark_group("create_nodes");
    for n in [1_000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                let mut dom = Dom::new();
                for _ in 0..n {
                    dom.create_element("div");
                }
            });
        });
    }
    group.finish();
}

fn bench_append_nodes(c: &mut Criterion) {
    c.bench_function("append_10000_nodes", |b| {
        b.iter(|| {
            let mut dom = Dom::new();
            let root = dom.root();
            for _ in 0..10_000 {
                let child = dom.create_element("div");
                dom.append_child(root, child);
            }
        });
    });
}

fn build_tree(dom: &mut Dom, parent: dom::NodeId, depth: u32, fanout: u32) {
    if depth == 0 {
        return;
    }
    for _ in 0..fanout {
        let child = dom.create_element("div");
        dom.append_child(parent, child);
        build_tree(dom, child, depth - 1, fanout);
    }
}

fn bench_remove_subtree(c: &mut Criterion) {
    c.bench_function("remove_subtree_depth6_fanout4", |b| {
        b.iter_batched(
            || {
                let mut dom = Dom::new();
                let root = dom.root();
                let subtree_root = dom.create_element("div");
                dom.append_child(root, subtree_root);
                build_tree(&mut dom, subtree_root, 6, 4);
                (dom, subtree_root)
            },
            |(mut dom, subtree_root)| {
                dom.remove_from_parent(subtree_root);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_clone_subtree(c: &mut Criterion) {
    c.bench_function("clone_subtree_deep_depth6_fanout4", |b| {
        b.iter_batched(
            || {
                let mut dom = Dom::new();
                let root = dom.root();
                let subtree_root = dom.create_element("div");
                dom.append_child(root, subtree_root);
                build_tree(&mut dom, subtree_root, 6, 4);
                (dom, subtree_root)
            },
            |(mut dom, subtree_root)| {
                dom.clone_node(subtree_root, true);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_find_by_id(c: &mut Criterion) {
    let mut dom = Dom::new();
    let root = dom.root();
    let mut target = root;
    for i in 0..10_000 {
        let child = dom.create_element("div");
        dom.set_attribute(child, "id", &format!("node-{i}"));
        dom.append_child(root, child);
        if i == 9_999 {
            target = child;
        }
    }
    let _ = target;
    c.bench_function("find_by_id_among_10000_last", |b| {
        b.iter(|| dom.find_by_id("node-9999"));
    });
}

fn bench_attribute_read_write(c: &mut Criterion) {
    let mut dom = Dom::new();
    let el = dom.create_element("div");
    c.bench_function("attribute_write_100000", |b| {
        b.iter(|| {
            for i in 0..100_000u32 {
                dom.set_attribute(el, "data-x", &i.to_string());
            }
        });
    });
    dom.set_attribute(el, "data-x", "0");
    c.bench_function("attribute_read_100000", |b| {
        b.iter(|| {
            for _ in 0..100_000u32 {
                let _ = dom.attribute(el, "data-x");
            }
        });
    });
}

criterion_group!(
    benches,
    bench_create_nodes,
    bench_append_nodes,
    bench_remove_subtree,
    bench_clone_subtree,
    bench_find_by_id,
    bench_attribute_read_write
);
criterion_main!(benches);
