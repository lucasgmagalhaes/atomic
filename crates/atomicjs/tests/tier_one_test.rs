//! @spec atomicjs-profiling#tier-one-lifecycle
use atomicjs::tier_one::TierOneFunction;
use atomicjs::Value;

fn compile_function(source: &str, name: &str) -> atomicjs::bytecode::BytecodeFunction {
    let tokens = atomicjs::lexer::tokenize(source).unwrap();
    let program = atomicjs::parser::parse(tokens).unwrap();
    let module = atomicjs::compiler::compile(program).unwrap();
    module
        .functions
        .into_iter()
        .find(|function| function.name.as_deref() == Some(name))
        .unwrap()
}

#[test]
fn numeric_loop_executes_in_specialized_tier() {
    let function = compile_function(
        "function sum(n) { let total = 0; for (let i = 0; i < n; i++) { total += i; } return total; }",
        "sum",
    );
    let tier_one = TierOneFunction::compile(&function);
    assert!(tier_one.is_some(), "unsupported bytecode: {function:#?}");
    let tier_one = tier_one.unwrap();
    let result = tier_one.run(&[Value::Number(10.0)]).unwrap();

    match result.value {
        Value::Number(value) => assert_eq!(value, 45.0),
        other => panic!("expected number, got {other:?}"),
    }
    assert_eq!(result.loop_backedges, 10);
}

#[test]
fn numeric_while_loop_executes_in_specialized_tier() {
    let function = compile_function(
        "function sum(n) { let total = 0; let i = 0; while (i < n) { total += i; i++; } return total; }",
        "sum",
    );
    let tier_one = TierOneFunction::compile(&function).expect("while loop must remain eligible");
    let result = tier_one.run(&[Value::Number(10.0)]).unwrap();
    assert!(matches!(result.value, Value::Number(45.0)));
    assert_eq!(result.loop_backedges, 10);
}

#[test]
fn guarded_property_access_is_tier_one_eligible() {
    let function = compile_function("function read(o) { return o.value; }", "read");
    assert!(TierOneFunction::compile(&function).is_some());
}

#[test]
fn numeric_tier_rejects_non_exact_or_nonnumeric_arguments_before_execution() {
    let function = compile_function("function add(left, right) { return left + right; }", "add");
    let tier_one = TierOneFunction::compile(&function).unwrap();

    assert!(tier_one.run(&[Value::Number(1.0)]).is_none());
    assert!(tier_one
        .run(&[Value::Number(1.0), Value::Undefined])
        .is_none());
    assert!(tier_one
        .run(&[Value::Number(1.0), Value::Number(2.0), Value::Number(3.0)])
        .is_none());
}

#[test]
fn numeric_tier_executes_supported_math_natives() {
    let function = compile_function(
        "function score(n) { return Math.sqrt(n) + Math.log(n); }",
        "score",
    );
    let tier_one = TierOneFunction::compile(&function).expect("math native must be eligible");
    let result = tier_one.run(&[Value::Number(9.0)]).unwrap();
    let Value::Number(value) = result.value else {
        panic!("expected numeric result");
    };
    assert_eq!(value, 3.0 + 9.0_f64.ln());
}

#[test]
fn tier_one_guards_a_numeric_object_property_read() {
    let function = compile_function("function read(player) { return player.damage; }", "read");
    let tier_one = TierOneFunction::compile(&function).expect("guarded property read is eligible");
    let object = std::rc::Rc::new(std::cell::RefCell::new(atomicjs::JsObject::default()));
    object.borrow_mut().set("damage", Value::Number(12.0));
    assert!(matches!(
        tier_one.run(&[Value::Object(object)]).unwrap().value,
        Value::Number(12.0)
    ));
    assert!(tier_one.run(&[Value::Number(12.0)]).is_none());
}
