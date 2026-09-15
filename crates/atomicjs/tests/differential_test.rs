//! Differential correctness gate for the intentionally supported AtomicJS
//! subset. QuickJS-ng is a development-only oracle: no production crate
//! depends on AtomicJS, and this test is never part of its public runtime API.

use atomicjs::run_source;
use quickjs_sys as quickjs;
use std::ffi::{CStr, CString};

fn quickjs_result(source: &str) -> String {
    let source = CString::new(source).expect("differential source must not contain NUL bytes");
    let filename = CString::new("atomicjs-differential.js").unwrap();
    let runtime = unsafe { quickjs::JS_NewRuntime() };
    assert!(!runtime.is_null(), "QuickJS failed to allocate a runtime");
    let context = unsafe { quickjs::JS_NewContext(runtime) };
    if context.is_null() {
        unsafe { quickjs::JS_FreeRuntime(runtime) };
        panic!("QuickJS failed to allocate a context");
    }

    let value = unsafe {
        quickjs::JS_Eval(
            context,
            source.as_ptr(),
            source.as_bytes().len(),
            filename.as_ptr(),
            quickjs::JS_EVAL_TYPE_GLOBAL,
        )
    };
    let result = if quickjs::js_is_exception(&value) {
        Err("QuickJS raised an exception for a supported differential program".to_string())
    } else {
        let mut length = 0;
        let raw_text = unsafe { quickjs::JS_ToCStringLen2(context, &mut length, value, false) };
        if raw_text.is_null() {
            Err("QuickJS could not stringify a differential result".to_string())
        } else {
            let text = unsafe { CStr::from_ptr(raw_text) }
                .to_string_lossy()
                .into_owned();
            unsafe { quickjs::JS_FreeCString(context, raw_text) };
            Ok(text)
        }
    };

    unsafe {
        quickjs::JS_FreeValue(context, value);
        quickjs::JS_FreeContext(context);
        quickjs::JS_FreeRuntime(runtime);
    }
    result.unwrap()
}

fn atomic_result(source: &str) -> Result<String, String> {
    run_source(source)
        .map(|value| value.to_string())
        .map_err(|error| error.to_string())
}

fn assert_matches_quickjs(name: &str, source: &str) {
    let atomic =
        atomic_result(source).unwrap_or_else(|error| panic!("AtomicJS failed {name}: {error}"));
    let quickjs = quickjs_result(source);
    assert_eq!(atomic, quickjs, "differential mismatch in {name}: {source}");
}

#[derive(Clone, Debug)]
struct GeneratedCase {
    start: u32,
    add: u32,
    multiplier: u32,
    divisor: u32,
    modulus: u32,
    threshold: u32,
    decrement: u32,
}

impl GeneratedCase {
    fn source(&self) -> String {
        format!(
            "let value = {start}; value = (value + {add}) * {multiplier}; value = value / {divisor} % {modulus}; if (value > {threshold}) {{ value = value - {decrement}; }} value;",
            start = self.start,
            add = self.add,
            multiplier = self.multiplier,
            divisor = self.divisor,
            modulus = self.modulus,
            threshold = self.threshold,
            decrement = self.decrement,
        )
    }
}

#[derive(Clone, Copy)]
enum CaseField {
    Start,
    Add,
    Multiplier,
    Divisor,
    Modulus,
    Threshold,
    Decrement,
}

impl CaseField {
    const ALL: [Self; 7] = [
        Self::Start,
        Self::Add,
        Self::Multiplier,
        Self::Divisor,
        Self::Modulus,
        Self::Threshold,
        Self::Decrement,
    ];

    fn value(self, case: &GeneratedCase) -> u32 {
        match self {
            Self::Start => case.start,
            Self::Add => case.add,
            Self::Multiplier => case.multiplier,
            Self::Divisor => case.divisor,
            Self::Modulus => case.modulus,
            Self::Threshold => case.threshold,
            Self::Decrement => case.decrement,
        }
    }

    fn set(self, case: &mut GeneratedCase, value: u32) {
        match self {
            Self::Start => case.start = value,
            Self::Add => case.add = value,
            Self::Multiplier => case.multiplier = value,
            Self::Divisor => case.divisor = value,
            Self::Modulus => case.modulus = value,
            Self::Threshold => case.threshold = value,
            Self::Decrement => case.decrement = value,
        }
    }

    fn minimum(self) -> u32 {
        match self {
            Self::Divisor | Self::Modulus => 1,
            _ => 0,
        }
    }
}

fn generated_cases(seed: u32, count: usize) -> impl Iterator<Item = GeneratedCase> {
    let mut state = seed;
    std::iter::repeat_with(move || {
        let next = |minimum: u32, maximum: u32, state: &mut u32| {
            *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            minimum + ((*state >> 16) % (maximum - minimum + 1))
        };
        GeneratedCase {
            start: next(0, 50, &mut state),
            add: next(0, 20, &mut state),
            multiplier: next(1, 10, &mut state),
            divisor: next(1, 10, &mut state),
            modulus: next(1, 17, &mut state),
            threshold: next(0, 10, &mut state),
            decrement: next(0, 5, &mut state),
        }
    })
    .take(count)
}

fn differs_from_quickjs(case: &GeneratedCase) -> bool {
    let source = case.source();
    atomic_result(&source) != Ok(quickjs_result(&source))
}

fn shrink_failing_case(case: &GeneratedCase) -> GeneratedCase {
    let mut smallest = case.clone();

    loop {
        let mut shrunk = false;
        for field in CaseField::ALL {
            let current = field.value(&smallest);
            for candidate in [field.minimum(), current / 2, 1] {
                if candidate == current || candidate < field.minimum() {
                    continue;
                }
                let mut proposal = smallest.clone();
                field.set(&mut proposal, candidate);
                if differs_from_quickjs(&proposal) {
                    smallest = proposal;
                    shrunk = true;
                    break;
                }
            }
            if shrunk {
                break;
            }
        }
        if !shrunk {
            return smallest;
        }
    }
}

fn assert_generated_case_matches_quickjs(seed: u32, index: usize, case: &GeneratedCase) {
    let source = case.source();
    let atomic = atomic_result(&source);
    let quickjs = quickjs_result(&source);
    if atomic == Ok(quickjs.clone()) {
        return;
    }

    let smallest = shrink_failing_case(case);
    panic!(
        "differential mismatch for seed {seed}, case {index}:\\noriginal: {source}\\nminimized: {}\\nAtomicJS: {atomic:?}\\nQuickJS: {quickjs}",
        smallest.source(),
    );
}

#[test]
fn fixed_supported_programs_match_quickjs() {
    let cases = [
        (
            "objects_and_members",
            "const player = { level: 10, damage: 20 }; player.level + player.damage;",
        ),
        (
            "closure_state",
            r#"
                function makeCounter() { let count = 0; return function () { return ++count; }; }
                const counter = makeCounter(); counter(); counter(); counter();
            "#,
        ),
        (
            "if_and_native_math",
            "let value = Math.sqrt(81) + Math.log(1); if (value > 8) { value *= 2; } value;",
        ),
        (
            "recursive_cross_function_call",
            r#"
                function cost(level) { if (level < 1) { return 10; } return cost(level - 1) * 1.15; }
                function affordable(budget, level) { let price = cost(level); if (price > budget) { return level; } return affordable(budget - price, level + 1); }
                affordable(1000, 0);
            "#,
        ),
    ];

    for (name, source) in cases {
        assert_matches_quickjs(name, source);
    }
}

#[test]
fn seeded_arithmetic_and_control_cases_match_quickjs() {
    const SEED: u32 = 0xA70C_1C5;
    for (index, case) in generated_cases(SEED, 64).enumerate() {
        assert_generated_case_matches_quickjs(SEED, index, &case);
    }
}
