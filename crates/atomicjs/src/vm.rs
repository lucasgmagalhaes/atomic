//! Interpreter loop, call frames, and execution — see
//! spec/proposals/ATOMIC_JS_SPIKE.md §5.4. Each JS call is a real, recursive
//! Rust function call (`execute_function` calling itself) — Rust's own call
//! stack backs the JS call stack directly, which is correct at this scope
//! (no manual multi-frame stack management needed) and matches §5.4's
//! `CallFrame` shape conceptually without the bookkeeping overhead of
//! maintaining an explicit frame list.

use std::cell::RefCell;
use std::rc::Rc;

use crate::bytecode::{BytecodeModule, Const, Instr};
use crate::error::AtomicJsError;
use crate::value::{FunctionData, FunctionFeedback, JsObject, Value};

/// A local slot is boxed (`Captured`) iff the compiler recorded it in its
/// owning function's `captured_locals` (i.e. some nested closure reads or
/// writes it) — everything else stays a cheap unboxed `Plain` value. This is
/// what keeps `sum`'s tight loop (nothing captured) from paying for
/// `Rc<RefCell<_>>` indirection it never needs, while `makeCounter`'s
/// `count` still works correctly once captured.
enum LocalSlot {
    Plain(Value),
    Captured(Rc<RefCell<Value>>),
}

impl LocalSlot {
    fn get(&self) -> Value {
        match self {
            LocalSlot::Plain(v) => v.clone(),
            LocalSlot::Captured(cell) => cell.borrow().clone(),
        }
    }

    fn set(&mut self, value: Value) {
        match self {
            LocalSlot::Plain(v) => *v = value,
            LocalSlot::Captured(cell) => *cell.borrow_mut() = value,
        }
    }
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Undefined => false,
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0.0,
        Value::Object(_) | Value::Function(_) => true,
    }
}

/// `ADD`/`LT` on a non-`Number` is unreachable for every one of the five
/// reference programs given a correct compiler — an internal invariant
/// violation, not a recoverable script error (§5.1's documented type-error
/// stance: panic, don't build a real type-error path this spike excludes).
fn as_number(value: &Value) -> f64 {
    match value {
        Value::Number(n) => *n,
        other => panic!(
            "internal error: expected Number, got {other:?} - ADD/LT on a \
             non-Number is unreachable for the spike's reference programs"
        ),
    }
}

pub fn run_source(source: &str) -> Result<Value, AtomicJsError> {
    run_source_with_feedback(source).map(|(value, _feedback)| value)
}

/// Same as `run_source`, but also returns the per-function
/// `FunctionFeedback` counters accumulated during this one run — exposed
/// specifically so a test (or a future caller) can confirm the
/// "collecting this is cheap and it's really happening" claim
/// spec/proposals/ATOMIC_JS_SPIKE.md §5.1 makes about them, without this
/// spike building any actual tiering consumer of the data.
pub fn run_source_with_feedback(
    source: &str,
) -> Result<(Value, Vec<FunctionFeedback>), AtomicJsError> {
    let tokens = crate::lexer::tokenize(source).map_err(|e| AtomicJsError(e.0))?;
    let program = crate::parser::parse(tokens).map_err(|e| AtomicJsError(e.0))?;
    let module = crate::compiler::compile(program).map_err(|e| AtomicJsError(e.0))?;

    let mut feedback: Vec<FunctionFeedback> = (0..module.functions.len())
        .map(|_| FunctionFeedback::default())
        .collect();
    let value = execute_function(
        &module,
        module.top_level,
        Vec::new(),
        Vec::new(),
        &mut feedback,
    )?;
    Ok((value, feedback))
}

fn execute_function(
    module: &BytecodeModule,
    function_index: usize,
    args: Vec<Value>,
    upvalues: Vec<Rc<RefCell<Value>>>,
    feedback: &mut [FunctionFeedback],
) -> Result<Value, AtomicJsError> {
    let function = &module.functions[function_index];
    feedback[function_index].call_count += 1;

    let mut locals: Vec<LocalSlot> = Vec::with_capacity(function.local_count);
    for i in 0..function.local_count {
        let initial = args.get(i).cloned().unwrap_or(Value::Undefined);
        if function.captured_locals.contains(&i) {
            locals.push(LocalSlot::Captured(Rc::new(RefCell::new(initial))));
        } else {
            locals.push(LocalSlot::Plain(initial));
        }
    }

    let mut stack: Vec<Value> = Vec::new();
    let mut pc: usize = 0;

    loop {
        match &function.code[pc] {
            Instr::LoadConst(idx) => {
                let value = match &function.constants[*idx as usize] {
                    Const::Number(n) => Value::Number(*n),
                    Const::Undefined => Value::Undefined,
                    // Never emitted by the compiler: `String` constants are
                    // only ever referenced by GetProp/SetProp's own index,
                    // never by LoadConst - an internal invariant, not a
                    // reachable script error.
                    Const::String(s) => panic!(
                        "internal error: LoadConst targeted a String constant ({s:?}) - \
                         the compiler never emits this"
                    ),
                };
                stack.push(value);
                pc += 1;
            }
            Instr::LoadLocal(slot) => {
                stack.push(locals[*slot as usize].get());
                pc += 1;
            }
            Instr::StoreLocal(slot) => {
                let value = stack.pop().expect("StoreLocal needs a value on the stack");
                locals[*slot as usize].set(value);
                pc += 1;
            }
            Instr::LoadUpvalue(idx) => {
                stack.push(upvalues[*idx as usize].borrow().clone());
                pc += 1;
            }
            Instr::StoreUpvalue(idx) => {
                let value = stack
                    .pop()
                    .expect("StoreUpvalue needs a value on the stack");
                *upvalues[*idx as usize].borrow_mut() = value;
                pc += 1;
            }
            Instr::GetProp(idx) => {
                // Borrowed, not cloned (fixed 2026-09-14 — see this file's
                // module doc / ATOMIC_JS_SPIKE.md's Results section): the
                // property name is a compile-time constant living in
                // `function.constants`, which already outlives this whole
                // call — cloning a fresh `String` (a heap allocation) on
                // every single GET_PROP was pure waste, confirmed by
                // profiling `props` with samply (~15% of inclusive time in
                // `String::clone`, mostly its own allocation).
                let name: &str = match &function.constants[*idx as usize] {
                    Const::String(s) => s.as_str(),
                    other => {
                        panic!("internal error: GetProp's constant must be a String, got {other:?}")
                    }
                };
                let object = stack.pop().expect("GetProp needs an object on the stack");
                let value = match object {
                    Value::Object(obj) => obj.borrow().get(name),
                    // Reading off a non-object stays total rather than
                    // panicking: none of the reference programs hit this,
                    // but nothing about GET_PROP's own semantics (§5.3)
                    // requires the operand to already be an object either.
                    _ => Value::Undefined,
                };
                stack.push(value);
                pc += 1;
            }
            Instr::SetProp(idx) => {
                let name = match &function.constants[*idx as usize] {
                    Const::String(s) => s.clone(),
                    other => {
                        panic!("internal error: SetProp's constant must be a String, got {other:?}")
                    }
                };
                let value = stack.pop().expect("SetProp needs a value on the stack");
                let object = stack
                    .last()
                    .expect("SetProp needs an object below the value")
                    .clone();
                match object {
                    Value::Object(obj) => obj.borrow_mut().set(name, value),
                    other => panic!(
                        "internal error: SetProp's target must be an object, got {other:?} - \
                         the compiler only ever emits SetProp for object-literal construction"
                    ),
                }
                pc += 1;
            }
            Instr::NewObject => {
                stack.push(Value::Object(Rc::new(RefCell::new(JsObject::default()))));
                pc += 1;
            }
            Instr::Add => {
                let b = stack.pop().expect("Add needs two operands");
                let a = stack.pop().expect("Add needs two operands");
                stack.push(Value::Number(as_number(&a) + as_number(&b)));
                pc += 1;
            }
            Instr::Lt => {
                let b = stack.pop().expect("Lt needs two operands");
                let a = stack.pop().expect("Lt needs two operands");
                stack.push(Value::Bool(as_number(&a) < as_number(&b)));
                pc += 1;
            }
            Instr::Jump(target) => {
                // A jump whose target is at or before this instruction's own
                // position is a loop back-edge - the cheap, sampled signal
                // `FunctionFeedback::loop_count` exists for (§5.1).
                if *target <= pc {
                    feedback[function_index].loop_count += 1;
                }
                pc = *target;
            }
            Instr::JumpIfFalse(target) => {
                let cond = stack
                    .pop()
                    .expect("JumpIfFalse needs a condition on the stack");
                if is_truthy(&cond) {
                    pc += 1;
                } else {
                    pc = *target;
                }
            }
            Instr::MakeClosure {
                function_index: target_index,
                captures,
            } => {
                let captured_env: Vec<Rc<RefCell<Value>>> = captures
                    .iter()
                    .map(|&slot| match &locals[slot as usize] {
                        LocalSlot::Captured(cell) => cell.clone(),
                        LocalSlot::Plain(_) => panic!(
                            "internal error: MakeClosure's captured slot {slot} was never \
                             marked Captured - compiler bug (captured_locals/captures mismatch)"
                        ),
                    })
                    .collect();
                stack.push(Value::Function(Rc::new(FunctionData {
                    function_index: *target_index as usize,
                    captured_env,
                })));
                pc += 1;
            }
            Instr::Call(argc) => {
                let callee = stack.pop().expect("Call needs a callee on the stack");
                let argc = *argc as usize;
                // Args were pushed left-to-right before the callee (compiler
                // convention, compiler.rs's `Expr::Call` arm), so popping
                // `argc` times yields them last-pushed-first; reverse to
                // restore the original left-to-right order.
                let mut args: Vec<Value> = (0..argc)
                    .map(|_| stack.pop().expect("Call needs argc values on the stack"))
                    .collect();
                args.reverse();
                let function_data = match callee {
                    Value::Function(f) => f,
                    other => {
                        return Err(AtomicJsError(format!(
                            "attempted to call a non-function value: {other}"
                        )))
                    }
                };
                let result = execute_function(
                    module,
                    function_data.function_index,
                    args,
                    function_data.captured_env.clone(),
                    feedback,
                )?;
                stack.push(result);
                pc += 1;
            }
            Instr::Return => {
                return Ok(stack.pop().unwrap_or(Value::Undefined));
            }
            Instr::Pop => {
                stack.pop();
                pc += 1;
            }
        }
    }
}
