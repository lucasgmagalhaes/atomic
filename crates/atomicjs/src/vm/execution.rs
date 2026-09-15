//! @spec atomicjs-profiling#modular-execution-core
//! Bytecode dispatch for one VM call frame.

use std::cell::RefCell;
use std::rc::Rc;

use crate::bytecode::{BytecodeModule, Const, Instr, NativeFn};
use crate::error::AtomicJsError;
use crate::value::{FunctionData, JsObject, Value};

use super::calls::{call_direct, call_local0, call_value};
use super::feedback::{FeedbackSink, TierOneState};
use super::object::{cached_local_property, cached_property, property_name, PropertyCache};
use super::operations::{as_number, binary_number, is_truthy, numeric_value};
use super::state::{LocalSlot, VmPools};

pub(super) fn execute_function<F: FeedbackSink>(
    module: &BytecodeModule,
    function_index: usize,
    args: &[Value],
    upvalues: &[Rc<RefCell<Value>>],
    feedback: &mut F,
    pools: &VmPools,
    tier_one: &mut TierOneState<'_>,
) -> Result<Value, AtomicJsError> {
    feedback.on_function_call(function_index);
    if let Some(Some(function)) = tier_one
        .functions
        .and_then(|functions| functions.get(function_index))
    {
        if let Some(result) =
            function.run_with_functions(args, upvalues, tier_one.functions.unwrap_or(&[]))
        {
            feedback.on_loop_backedges(function_index, result.loop_backedges);
            *tier_one.calls = tier_one.calls.saturating_add(1);
            return Ok(result.value);
        }
        *tier_one.fallbacks = tier_one.fallbacks.saturating_add(1);
    }
    let function = &module.functions[function_index];
    let mut locals = pools.take_locals();
    for i in 0..function.local_count {
        let initial = args.get(i).cloned().unwrap_or(Value::Undefined);
        locals.push(if function.captured_locals.contains(&i) {
            LocalSlot::Captured(Rc::new(RefCell::new(initial)))
        } else {
            LocalSlot::Plain(initial)
        });
    }
    let mut stack = pools.take_stack();
    let mut property_caches = function
        .has_property_reads
        .then(|| vec![None::<PropertyCache>; function.code.len()]);
    let mut pc = 0;
    loop {
        match &function.code[pc] {
            Instr::LoadConst(idx) => {
                stack.push(match &function.constants[*idx as usize] {
                    Const::Number(n) => Value::Number(*n),
                    Const::Undefined => Value::Undefined,
                    Const::String(s) => panic!("internal error: LoadConst targeted String {s:?}"),
                });
                pc += 1;
            }
            Instr::LoadLocal(slot) => {
                stack.push(locals[*slot as usize].get());
                pc += 1;
            }
            Instr::StoreLocal(slot) => {
                locals[*slot as usize].set(stack.pop().expect("StoreLocal needs a value"));
                pc += 1;
            }
            Instr::LoadUpvalue(idx) => {
                stack.push(upvalues[*idx as usize].borrow().clone());
                pc += 1;
            }
            Instr::StoreUpvalue(idx) => {
                *upvalues[*idx as usize].borrow_mut() =
                    stack.pop().expect("StoreUpvalue needs a value");
                pc += 1;
            }
            Instr::GetProp(idx) => {
                let name = match &function.constants[*idx as usize] {
                    Const::String(s) => s.as_str(),
                    other => panic!("internal error: GetProp requires String, got {other:?}"),
                };
                let value = match stack.pop().expect("GetProp needs an object") {
                    Value::Object(object) => object.borrow().get(name),
                    _ => Value::Undefined,
                };
                stack.push(value);
                pc += 1;
            }
            Instr::GetLocalProp { local, name } => {
                let cache = property_caches.as_mut().map(|entries| &mut entries[pc]);
                stack.push(cached_local_property(
                    &locals[*local as usize],
                    property_name(function, *name),
                    cache,
                ));
                pc += 1;
            }
            Instr::GetUpvalueProp { upvalue, name } => {
                let value = upvalues[*upvalue as usize].borrow();
                let property = match &*value {
                    Value::Object(object) => cached_property(
                        object,
                        property_name(function, *name),
                        property_caches.as_mut().map(|entries| &mut entries[pc]),
                    ),
                    _ => Value::Undefined,
                };
                stack.push(property);
                pc += 1;
            }
            Instr::SetProp(idx) => {
                let name = match &function.constants[*idx as usize] {
                    Const::String(s) => s.clone(),
                    other => panic!("internal error: SetProp requires String, got {other:?}"),
                };
                let value = stack.pop().expect("SetProp needs a value");
                match stack.last().expect("SetProp needs an object").clone() {
                    Value::Object(object) => object.borrow_mut().set(name, value),
                    other => panic!("internal error: SetProp target must be object, got {other:?}"),
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
            Instr::AddLocalLocal { target, value } => {
                let sum = locals[*target as usize].number() + locals[*value as usize].number();
                locals[*target as usize].set_number(sum);
                pc += 1;
            }
            Instr::AddLocalConst { target, constant } => {
                let constant = match &function.constants[*constant as usize] {
                    Const::Number(value) => *value,
                    other => panic!("internal error: AddLocalConst requires Number, got {other:?}"),
                };
                let sum = locals[*target as usize].number() + constant;
                locals[*target as usize].set_number(sum);
                pc += 1;
            }
            Instr::AddLocalProp {
                target,
                object,
                name,
            } => {
                let property = cached_local_property(
                    &locals[*object as usize],
                    property_name(function, *name),
                    property_caches.as_mut().map(|entries| &mut entries[pc]),
                );
                let sum = locals[*target as usize].number() + as_number(&property);
                locals[*target as usize].set_number(sum);
                pc += 1;
            }
            Instr::AddLocalCallLocal0 { target, callee } => {
                let total = locals[*target as usize].number();
                let result =
                    call_local0(module, &locals[*callee as usize], feedback, pools, tier_one)?;
                locals[*target as usize].set_number(total + as_number(&result));
                pc += 1;
            }
            Instr::BinaryLocalLocal { op, left, right } => {
                stack.push(numeric_value(
                    *op,
                    locals[*left as usize].number(),
                    locals[*right as usize].number(),
                ));
                pc += 1;
            }
            Instr::BinaryLocalConst {
                op,
                local,
                constant,
            } => {
                let constant = match &function.constants[*constant as usize] {
                    Const::Number(number) => *number,
                    other => {
                        panic!("internal error: BinaryLocalConst requires Number, got {other:?}")
                    }
                };
                stack.push(numeric_value(
                    *op,
                    locals[*local as usize].number(),
                    constant,
                ));
                pc += 1;
            }
            Instr::IncrementLocal(slot) => {
                let value = locals[*slot as usize].number() + 1.0;
                locals[*slot as usize].set_number(value);
                pc += 1;
            }
            Instr::IncrementUpvalue(upvalue) => {
                let mut value = upvalues[*upvalue as usize].borrow_mut();
                let incremented = as_number(&value) + 1.0;
                *value = Value::Number(incremented);
                stack.push(Value::Number(incremented));
                pc += 1;
            }
            Instr::Sub => {
                binary_number(&mut stack, |a, b| a - b);
                pc += 1;
            }
            Instr::Mul => {
                binary_number(&mut stack, |a, b| a * b);
                pc += 1;
            }
            Instr::Div => {
                binary_number(&mut stack, |a, b| a / b);
                pc += 1;
            }
            Instr::Mod => {
                binary_number(&mut stack, |a, b| a % b);
                pc += 1;
            }
            Instr::Lt => {
                let b = stack.pop().expect("Lt needs two operands");
                let a = stack.pop().expect("Lt needs two operands");
                stack.push(Value::Bool(as_number(&a) < as_number(&b)));
                pc += 1;
            }
            Instr::Jump(target) => {
                if *target <= pc {
                    feedback.on_loop_backedge(function_index);
                }
                pc = *target;
            }
            Instr::JumpIfFalse(target) => {
                let condition = stack.pop().expect("JumpIfFalse needs a condition");
                pc = if is_truthy(&condition) {
                    pc + 1
                } else {
                    *target
                };
            }
            Instr::MakeClosure {
                function_index: target,
                captures,
            } => {
                let captured_env = captures
                    .iter()
                    .map(|&slot| match &locals[slot as usize] {
                        LocalSlot::Captured(cell) => cell.clone(),
                        LocalSlot::Plain(_) => {
                            panic!("internal error: closure capture was not boxed")
                        }
                    })
                    .collect();
                stack.push(Value::Function(Rc::new(FunctionData {
                    function_index: *target as usize,
                    captured_env,
                })));
                pc += 1;
            }
            Instr::Call(argc) => {
                let result = call_value(
                    module,
                    &mut stack,
                    *argc as usize,
                    feedback,
                    pools,
                    tier_one,
                )?;
                stack.push(result);
                pc += 1;
            }
            Instr::CallDirect {
                function_index,
                argc,
            } => {
                let result = call_direct(
                    module,
                    &mut stack,
                    *function_index as usize,
                    *argc as usize,
                    feedback,
                    pools,
                    tier_one,
                )?;
                stack.push(result);
                pc += 1;
            }
            Instr::CallLocal0(local) => {
                stack.push(call_local0(
                    module,
                    &locals[*local as usize],
                    feedback,
                    pools,
                    tier_one,
                )?);
                pc += 1;
            }
            Instr::CallNative(native) => {
                let value = as_number(&stack.pop().expect("CallNative needs one operand"));
                stack.push(Value::Number(match native {
                    NativeFn::Sqrt => value.sqrt(),
                    NativeFn::Log => value.ln(),
                }));
                pc += 1;
            }
            Instr::Return => {
                let result = stack.pop().unwrap_or(Value::Undefined);
                pools.return_stack(stack);
                pools.return_locals(locals);
                return Ok(result);
            }
            Instr::Pop => {
                stack.pop();
                pc += 1;
            }
        }
    }
}
