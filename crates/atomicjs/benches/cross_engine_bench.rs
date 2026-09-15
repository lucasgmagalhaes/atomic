//! Reproducible hot-function comparison between AtomicJS Tier 1 and QuickJS-ng.
//!
//! Both engines parse and install the same `sum` fixture once before Criterion
//! samples. Each sample then executes `sum(1_000_000)` without constructing a
//! runtime, context, or source string. AtomicJS retains its public
//! compile-once/run-many boundary; QuickJS-ng calls the already-installed
//! global function through its native API.

// @spec atomicjs-profiling#cross-engine-benchmark
use atomicjs::tiering::TieringPolicy;
use atomicjs::{TieredProgram, Value};
use criterion::{criterion_group, criterion_main, Criterion};
use quickjs_sys as quickjs;
use std::ffi::CString;

const SUM: &str = include_str!("../benchmarks/scripts/sum.js");
const EXPECTED_SUM: f64 = 499_999_500_000.0;
const NUMERIC_CALL_GRAPH: &str = r#"
function increment(n) { return n + 1; }
function accumulate(n) {
    let total = 0;
    for (let i = 0; i < n; i++) { total += increment(i); }
    return total;
}
accumulate(1000000);
"#;
const EXPECTED_CALL_GRAPH: f64 = 500_000_500_000.0;

fn bench_hot_sum(criterion: &mut Criterion) {
    let mut atomic = TieredProgram::compile(
        SUM,
        TieringPolicy {
            enabled: true,
            call_threshold: 1,
            loop_threshold: 1,
            ..TieringPolicy::default()
        },
    )
    .unwrap();
    let warmup = atomic.run().unwrap();
    assert!(matches!(warmup.value, Value::Number(value) if value == EXPECTED_SUM));
    let accelerated = atomic.run().unwrap();
    assert!(matches!(accelerated.value, Value::Number(value) if value == EXPECTED_SUM));
    assert!(accelerated.tier_one_calls > 0);

    let mut quickjs = QuickJsFunction::new(SUM, "sum");
    assert_eq!(quickjs.call(), EXPECTED_SUM);

    criterion.bench_function("cross_engine/sum_atomicjs_tier_one_hot", |bench| {
        bench.iter(|| atomic.run().unwrap())
    });
    criterion.bench_function("cross_engine/sum_quickjs_hot", |bench| {
        bench.iter(|| quickjs.call())
    });

    let mut atomic_graph = TieredProgram::compile(
        NUMERIC_CALL_GRAPH,
        TieringPolicy {
            enabled: true,
            call_threshold: 1,
            loop_threshold: 1,
            ..TieringPolicy::default()
        },
    )
    .unwrap();
    atomic_graph.run().unwrap();
    let accelerated = atomic_graph.run().unwrap();
    assert!(matches!(accelerated.value, Value::Number(value) if value == EXPECTED_CALL_GRAPH));
    assert!(accelerated.tier_one_calls > 0);

    let mut quickjs_graph = QuickJsFunction::new(NUMERIC_CALL_GRAPH, "accumulate");
    assert_eq!(quickjs_graph.call(), EXPECTED_CALL_GRAPH);
    criterion.bench_function("cross_engine/call_graph_atomicjs_tier_one_hot", |bench| {
        bench.iter(|| atomic_graph.run().unwrap())
    });
    criterion.bench_function("cross_engine/call_graph_quickjs_hot", |bench| {
        bench.iter(|| quickjs_graph.call())
    });
}

struct QuickJsFunction {
    runtime: *mut quickjs::JSRuntime,
    context: *mut quickjs::JSContext,
    function: quickjs::JSValue,
}

impl QuickJsFunction {
    fn new(source: &str, function_name: &str) -> Self {
        let source = CString::new(source).unwrap();
        let filename = CString::new("atomicjs-cross-engine.js").unwrap();
        unsafe {
            let runtime = quickjs::JS_NewRuntime();
            assert!(!runtime.is_null(), "QuickJS-ng runtime allocation failed");
            let context = quickjs::JS_NewContext(runtime);
            assert!(!context.is_null(), "QuickJS-ng context allocation failed");
            let value = quickjs::JS_Eval(
                context,
                source.as_ptr(),
                source.as_bytes().len(),
                filename.as_ptr(),
                quickjs::JS_EVAL_TYPE_GLOBAL,
            );
            assert!(
                !quickjs::js_is_exception(&value),
                "QuickJS-ng failed to install the shared sum fixture"
            );
            quickjs::JS_FreeValue(context, value);

            let global = quickjs::JS_GetGlobalObject(context);
            let name = CString::new(function_name).unwrap();
            let function = quickjs::JS_GetPropertyStr(context, global, name.as_ptr());
            quickjs::JS_FreeValue(context, global);
            assert!(quickjs::JS_IsFunction(context, function));
            Self {
                runtime,
                context,
                function,
            }
        }
    }

    fn call(&mut self) -> f64 {
        let mut argument = quickjs::js_float64(1_000_000.0);
        unsafe {
            let result = quickjs::JS_Call(
                self.context,
                self.function,
                quickjs::js_undefined(),
                1,
                &mut argument,
            );
            assert!(
                !quickjs::js_is_exception(&result),
                "QuickJS-ng direct call failed"
            );
            assert_eq!(result.tag, quickjs::JS_TAG_FLOAT64);
            let value = result.u.float64;
            quickjs::JS_FreeValue(self.context, result);
            value
        }
    }
}

impl Drop for QuickJsFunction {
    fn drop(&mut self) {
        unsafe {
            quickjs::JS_FreeValue(self.context, self.function);
            quickjs::JS_FreeContext(self.context);
            quickjs::JS_FreeRuntime(self.runtime);
        }
    }
}

criterion_group!(benches, bench_hot_sum);
criterion_main!(benches);
