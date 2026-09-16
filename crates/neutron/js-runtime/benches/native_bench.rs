//! Real JS↔native FFI boundary benchmarks — `spec/architecture/
//! performance.md` §21.2's suite. Each measures 100,000 real crossings
//! into a native `js-runtime` binding (attribute get/set, classList
//! mutation, dataset access, event dispatch) from one `ctx.eval` loop —
//! the cost is dominated by the native-call boundary itself, not
//! interpreter overhead (see `hot_loop.rs`'s own benchmark for pure
//! interpreter throughput with no native calls at all, for comparison).
use criterion::{criterion_group, criterion_main, Criterion};
use js_runtime::{Context, Runtime};

fn setup_element(ctx: &Context) {
    ctx.eval(
        "globalThis.el = document.createElement('div'); \
         el.id = 'bench-el';",
        "<bench setup>",
    )
    .expect("setup must not throw");
}

fn bench_attribute_write(c: &mut Criterion) {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    setup_element(&ctx);
    c.bench_function("js_native_attribute_write_100000", |b| {
        b.iter(|| {
            ctx.eval(
                "for (let i = 0; i < 100000; i++) { el.setAttribute('data-x', String(i)); }",
                "<bench>",
            )
            .expect("must not throw");
        });
    });
}

fn bench_attribute_read(c: &mut Criterion) {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    setup_element(&ctx);
    ctx.eval("el.setAttribute('data-x', 'v')", "<bench setup>")
        .unwrap();
    c.bench_function("js_native_attribute_read_100000", |b| {
        b.iter(|| {
            ctx.eval(
                "let s = 0; for (let i = 0; i < 100000; i++) { s += el.getAttribute('data-x').length; }",
                "<bench>",
            )
            .expect("must not throw");
        });
    });
}

fn bench_class_list_mutations(c: &mut Criterion) {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    setup_element(&ctx);
    c.bench_function("js_native_class_list_mutations_100000", |b| {
        b.iter(|| {
            ctx.eval(
                "for (let i = 0; i < 100000; i++) { el.classList.add('x'); el.classList.remove('x'); }",
                "<bench>",
            )
            .expect("must not throw");
        });
    });
}

fn bench_dataset_access(c: &mut Criterion) {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    setup_element(&ctx);
    c.bench_function("js_native_dataset_access_100000", |b| {
        b.iter(|| {
            ctx.eval(
                "for (let i = 0; i < 100000; i++) { el.dataset.x = String(i); let _ = el.dataset.x; }",
                "<bench>",
            )
            .expect("must not throw");
        });
    });
}

fn bench_event_dispatch(c: &mut Criterion) {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    setup_element(&ctx);
    ctx.eval(
        "let count = 0; el.addEventListener('click', () => { count++; });",
        "<bench setup>",
    )
    .unwrap();
    c.bench_function("js_native_event_dispatch_100000", |b| {
        b.iter(|| {
            ctx.eval(
                "for (let i = 0; i < 100000; i++) { el.dispatchEvent('click'); }",
                "<bench>",
            )
            .expect("must not throw");
        });
    });
}

fn bench_element_creation(c: &mut Criterion) {
    c.bench_function("js_native_create_element_100000", |b| {
        b.iter(|| {
            let rt = Runtime::new();
            let ctx = Context::with_dom(&rt, dom::Dom::new());
            ctx.eval(
                "for (let i = 0; i < 100000; i++) { document.createElement('div'); }",
                "<bench>",
            )
            .expect("must not throw");
        });
    });
}

criterion_group!(
    benches,
    bench_attribute_write,
    bench_attribute_read,
    bench_class_list_mutations,
    bench_dataset_access,
    bench_event_dispatch,
    bench_element_creation
);
criterion_main!(benches);
