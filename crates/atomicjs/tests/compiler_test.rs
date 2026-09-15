use atomicjs::bytecode::{Const, Instr};
use atomicjs::compiler::compile;
use atomicjs::lexer::tokenize;
use atomicjs::parser::parse;

mod common;

fn compile_source(source: &str) -> atomicjs::bytecode::BytecodeModule {
    let tokens = tokenize(source).unwrap();
    let program = parse(tokens).unwrap();
    compile(program).unwrap()
}

#[test]
fn compiles_every_reference_program_without_error() {
    for (name, source) in common::all() {
        let tokens =
            tokenize(source).unwrap_or_else(|e| panic!("failed to tokenize {name:?}: {e:?}"));
        let program = parse(tokens).unwrap_or_else(|e| panic!("failed to parse {name:?}: {e:?}"));
        compile(program).unwrap_or_else(|e| panic!("failed to compile {name:?}: {e:?}"));
    }
}

#[test]
fn sum_compiles_to_two_functions_sum_and_top_level() {
    let module = compile_source(common::SUM);
    // function[0] = sum (declared first, compiled first); function[1] =
    // the top-level script itself (finishes compiling last, since it has to
    // finish compiling its own body — including the `sum` declaration —
    // before compile_function appends it to `functions`).
    assert_eq!(module.functions.len(), 2);
    assert_eq!(module.top_level, 1);
    assert_eq!(module.functions[0].name.as_deref(), Some("sum"));
    assert_eq!(module.functions[0].param_count, 1);
    assert!(module.functions[0].captured_locals.is_empty());
}

#[test]
fn closure_program_produces_inner_makecounter_and_top_level_with_capture_wiring() {
    let module = compile_source(common::CLOSURE);
    // function[0] = the anonymous inner closure (compiled first, while
    // compiling makeCounter's body); function[1] = makeCounter itself;
    // function[2] = the top-level script.
    assert_eq!(module.functions.len(), 3);
    assert_eq!(module.top_level, 2);

    let inner = &module.functions[0];
    assert!(inner.name.is_none());
    assert!(
        inner.code.contains(&Instr::LoadUpvalue(0)),
        "inner closure should read `count` via LoadUpvalue(0), got: {:?}",
        inner.code
    );

    let make_counter = &module.functions[1];
    assert_eq!(make_counter.name.as_deref(), Some("makeCounter"));
    // `count` is makeCounter's only local (no params) -> slot 0, and it's
    // captured by the inner closure.
    assert_eq!(make_counter.captured_locals, vec![0]);
    assert!(
        make_counter.code.iter().any(|i| matches!(
            i,
            Instr::MakeClosure { function_index: 0, captures } if captures == &vec![0]
        )),
        "makeCounter should emit MakeClosure{{function_index: 0, captures: [0]}}, got: {:?}",
        make_counter.code
    );
}

#[test]
fn object_literal_compiles_to_new_object_then_set_prop_per_field() {
    let module = compile_source("const player = { level: 10, damage: 20 };");
    let top_level = &module.functions[module.top_level];
    assert!(matches!(top_level.code[0], Instr::NewObject));
    let set_prop_count = top_level
        .code
        .iter()
        .filter(|i| matches!(i, Instr::SetProp(_)))
        .count();
    assert_eq!(set_prop_count, 2);
}

#[test]
fn member_read_from_a_local_uses_the_direct_property_opcode() {
    let module = compile_source("const player = { level: 10 }; player.level;");
    let top_level = &module.functions[module.top_level];
    assert!(top_level
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::GetLocalProp { .. })));
    assert!(!top_level
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::GetProp(_))));
}

#[test]
fn zero_argument_local_call_uses_the_direct_call_opcode() {
    let module = compile_source("function f() { return 1; } f();");
    let top_level = &module.functions[module.top_level];
    assert!(top_level
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::CallLocal0(_))));
}

#[test]
fn discarded_local_arithmetic_updates_use_direct_opcodes() {
    let module = compile_source("let total = 0; let i = 1; total += i; i++; 0;");
    let top_level = &module.functions[module.top_level];
    assert!(top_level
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::AddLocalLocal { .. })));
    assert!(top_level
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::IncrementLocal(_))));
}

#[test]
fn discarded_local_property_accumulation_uses_a_direct_opcode() {
    let module = compile_source(
        "let total = 0; const player = { damage: 20 }; total += player.damage; 0;",
    );
    let top_level = &module.functions[module.top_level];
    assert!(top_level
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::AddLocalProp { .. })));
    assert!(!top_level
        .code
        .iter()
        .any(|instr| matches!(instr, Instr::GetLocalProp { .. })));
}

#[test]
fn for_loop_compiles_a_backward_jump_and_a_patched_forward_jump() {
    let module = compile_source(
        r#"
        for (let i = 0; i < 10; i++) {
            i;
        }
        "#,
    );
    let top_level = &module.functions[module.top_level];
    let has_backward_jump = top_level
        .code
        .iter()
        .any(|i| matches!(i, Instr::Jump(target) if *target < top_level.code.len()));
    assert!(
        has_backward_jump,
        "expected a Jump back to the loop condition, got: {:?}",
        top_level.code
    );

    let jump_if_false_target = top_level.code.iter().find_map(|i| match i {
        Instr::JumpIfFalse(target) => Some(*target),
        _ => None,
    });
    assert!(
        jump_if_false_target.is_some() && jump_if_false_target.unwrap() != usize::MAX,
        "JumpIfFalse's placeholder target should have been patched to a real pc, got: {:?}",
        top_level.code
    );
}

#[test]
fn discards_assignment_values_without_reloading_them() {
    let module = compile_source("let i = 0; i++; 0;");
    let top_level = &module.functions[module.top_level];
    assert!(
        !top_level.code.windows(3).any(|window| matches!(
            window,
            [Instr::Add, Instr::StoreLocal(_), Instr::LoadLocal(_)]
                | [Instr::Add, Instr::StoreUpvalue(_), Instr::LoadUpvalue(_)]
        )),
        "discarded assignment should not reload a value only to pop it: {:?}",
        top_level.code
    );
}

#[test]
fn rejects_assignment_target_that_is_not_an_identifier() {
    // Not reachable from any of the five reference programs, but the
    // compiler must still fail closed rather than silently mis-compile.
    let tokens = tokenize("player.damage += 1;").unwrap();
    let program = parse(tokens).unwrap();
    assert!(compile(program).is_err());
}

#[test]
fn rejects_reference_to_an_undeclared_variable() {
    let tokens = tokenize("doesNotExist;").unwrap();
    let program = parse(tokens).unwrap();
    assert!(compile(program).is_err());
}

#[test]
fn constant_pool_carries_number_and_string_and_undefined_kinds() {
    let module = compile_source("let x = 1; return;");
    let top_level = &module.functions[module.top_level];
    assert!(top_level.constants.contains(&Const::Number(1.0)));
    assert!(top_level.constants.contains(&Const::Undefined));

    let module = compile_source("const player = { level: 10 };");
    let top_level = &module.functions[module.top_level];
    assert!(top_level
        .constants
        .contains(&Const::String("level".to_string())));
}
