//! @spec atomicjs-profiling#tier-one-policy
//! Interpreter loop, call frames, and execution — see
//! spec/proposals/ATOMIC_JS_SPIKE.md §5.4. Each JS call is a real, recursive
//! Rust function call (`execute_function` calling itself) — Rust's own call
//! stack backs the JS call stack directly, which is correct at this scope
//! (no manual multi-frame stack management needed) and matches §5.4's
//! `CallFrame` shape conceptually without the bookkeeping overhead of
//! maintaining an explicit frame list.

use std::cell::RefCell;
use std::rc::Rc;

use crate::bytecode::{BytecodeFunction, BytecodeModule, Const, Instr, NativeFn, NumericOp};
use crate::error::AtomicJsError;
use crate::tiering::{HostState, TierDecision, TieringController, TieringPolicy};
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

#[derive(Clone, Copy)]
struct PropertyCache {
    shape_id: u64,
    slot: usize,
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

    #[inline(always)]
    fn number(&self) -> f64 {
        match self {
            LocalSlot::Plain(Value::Number(number)) => *number,
            LocalSlot::Captured(cell) => as_number(&cell.borrow()),
            LocalSlot::Plain(value) => as_number(value),
        }
    }

    #[inline(always)]
    fn set_number(&mut self, number: f64) {
        match self {
            LocalSlot::Plain(Value::Number(value)) => *value = number,
            LocalSlot::Captured(cell) => *cell.borrow_mut() = Value::Number(number),
            LocalSlot::Plain(value) => *value = Value::Number(number),
        }
    }
}

/// Reusable `Vec<LocalSlot>`/`Vec<Value>` buffers shared across the whole
/// recursive call tree — added 2026-09-14 after profiling `closures` with
/// `samply` (see ATOMIC_JS_SPIKE.md's Results section): `execute_function`
/// previously allocated a fresh `Vec` for locals and for the operand stack
/// on *every single call*, and `closures`' loop calls the same closure
/// 1,000,000 times, so that was 2,000,000+ heap allocations attributable
/// purely to the calling convention. A finished call's buffers (cleared, not
/// deallocated) go back into the pool for the next call to reuse instead.
///
/// `RefCell`, not a `&mut` threaded through every recursive call: the pool
/// needs to be reachable from arbitrarily deep, already-nested
/// `execute_function` calls (a call in progress still holds its own
/// buffers checked out), and a shared reference is simpler to thread
/// through than a mutable-borrow chain — same tradeoff `compiler.rs`
/// already made for its own `Scope` handle, and just as fine here: this is
/// interpreter bookkeeping, not something with its own correctness-critical
/// aliasing concerns.
#[derive(Default)]
struct VmPools {
    locals: RefCell<Vec<Vec<LocalSlot>>>,
    stack: RefCell<Vec<Vec<Value>>>,
    /// Added after re-profiling post-locals/stack-pooling: `Instr::Call`
    /// still collected popped arguments into a fresh `Vec<Value>` on every
    /// call before handing them to the callee — the next-biggest allocation
    /// left, visible in `closures`' profile as `Vec::from_iter` + a handful
    /// of `malloc` frames. Same pooling treatment as `locals`/`stack`.
    args: RefCell<Vec<Vec<Value>>>,
}

/// Keeps profiling out of the normal execution path. `run_source` uses the
/// zero-sized `NoFeedback` implementation, which LLVM can inline away;
/// `run_source_with_feedback` opts into exact counters for tests and tooling.
trait FeedbackSink {
    fn on_function_call(&mut self, function_index: usize);
    fn on_loop_backedge(&mut self, function_index: usize);
}

struct NoFeedback;

impl FeedbackSink for NoFeedback {
    #[inline(always)]
    fn on_function_call(&mut self, _: usize) {}

    #[inline(always)]
    fn on_loop_backedge(&mut self, _: usize) {}
}

struct CollectingFeedback<'a> {
    entries: &'a mut [FunctionFeedback],
}

impl FeedbackSink for CollectingFeedback<'_> {
    #[inline(always)]
    fn on_function_call(&mut self, function_index: usize) {
        self.entries[function_index].call_count += 1;
    }

    #[inline(always)]
    fn on_loop_backedge(&mut self, function_index: usize) {
        self.entries[function_index].loop_count += 1;
    }
}

impl VmPools {
    fn take_locals(&self) -> Vec<LocalSlot> {
        self.locals.borrow_mut().pop().unwrap_or_default()
    }

    fn take_stack(&self) -> Vec<Value> {
        self.stack.borrow_mut().pop().unwrap_or_default()
    }

    /// Only called from the success path (`Instr::Return`, see
    /// `execute_function`) - an error path drops its frame's buffers
    /// normally instead of pooling them, which is fine: errors aren't the
    /// hot path this pool exists for.
    fn return_locals(&self, mut v: Vec<LocalSlot>) {
        v.clear();
        self.locals.borrow_mut().push(v);
    }

    fn return_stack(&self, mut v: Vec<Value>) {
        v.clear();
        self.stack.borrow_mut().push(v);
    }

    fn take_args(&self) -> Vec<Value> {
        self.args.borrow_mut().pop().unwrap_or_default()
    }

    fn return_args(&self, mut v: Vec<Value>) {
        v.clear();
        self.args.borrow_mut().push(v);
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
    CompiledProgram::compile(source)?.run()
}

/// Immutable bytecode compiled from one AtomicJS source string. Each
/// [`Self::run`] starts a new top-level frame, so locals, closures, and
/// objects never leak state from an earlier execution.
#[derive(Debug)]
pub struct CompiledProgram {
    module: BytecodeModule,
}

impl CompiledProgram {
    /// Parses and compiles `source` once for repeated isolated executions.
    pub fn compile(source: &str) -> Result<Self, AtomicJsError> {
        Ok(Self {
            module: compile_module(source)?,
        })
    }

    /// Executes the compiled bytecode with a fresh VM state and no profiling
    /// instrumentation in the hot path.
    pub fn run(&self) -> Result<Value, AtomicJsError> {
        let pools = VmPools::default();
        let mut feedback = NoFeedback;
        execute_function(
            &self.module,
            self.module.top_level,
            &[],
            &[],
            &mut feedback,
            &pools,
        )
    }

    /// Executes with per-function feedback, keeping instrumentation opt-in.
    pub fn run_with_feedback(&self) -> Result<(Value, Vec<FunctionFeedback>), AtomicJsError> {
        let mut feedback: Vec<FunctionFeedback> = (0..self.module.functions.len())
            .map(|_| FunctionFeedback::default())
            .collect();
        let pools = VmPools::default();
        let mut collector = CollectingFeedback {
            entries: &mut feedback,
        };
        let value = execute_function(
            &self.module,
            self.module.top_level,
            &[],
            &[],
            &mut collector,
            &pools,
        )?;
        Ok((value, feedback))
    }
}

/// Result of one [`TieredProgram`] execution. `decisions` is indexed exactly
/// like the compiled module's functions; `Eligible` means the function is hot
/// and has been admitted under the configured Tier 1 code budget.
#[derive(Debug)]
pub struct TieredRun {
    pub value: Value,
    pub decisions: Vec<TierDecision>,
}

/// Compile-once, run-many boundary with persistent tiering feedback.
///
/// This first isolated tier intentionally still executes every call through
/// the interpreter. It makes promotion, pause invalidation, and code-budget
/// admission observable and testable while retaining the interpreter as the
/// semantic fallback. A later executable backend can consume `Eligible`
/// functions without changing this public contract.
pub struct TieredProgram {
    program: CompiledProgram,
    controller: TieringController,
    estimated_code_bytes: Vec<usize>,
}

impl TieredProgram {
    pub fn compile(source: &str, policy: TieringPolicy) -> Result<Self, AtomicJsError> {
        let program = CompiledProgram::compile(source)?;
        let estimated_code_bytes = program
            .module
            .functions
            .iter()
            .map(estimate_tier_one_bytes)
            .collect();
        let controller = TieringController::new(policy, program.module.functions.len());
        Ok(Self {
            program,
            controller,
            estimated_code_bytes,
        })
    }

    pub fn set_host_state(&mut self, host: HostState) {
        self.controller.set_host_state(host);
    }

    pub fn run(&mut self) -> Result<TieredRun, AtomicJsError> {
        let (value, feedback) = self.program.run_with_feedback()?;
        let decisions = self
            .controller
            .observe(&feedback, &self.estimated_code_bytes);
        Ok(TieredRun { value, decisions })
    }
}

/// Conservative, allocation-free admission estimate. It bounds each
/// instruction and constant at the Rust representation size, avoiding a
/// pretend precise code-size claim before there is generated native code.
fn estimate_tier_one_bytes(function: &BytecodeFunction) -> usize {
    function
        .code
        .len()
        .saturating_mul(std::mem::size_of::<Instr>())
        .saturating_add(
            function
                .constants
                .len()
                .saturating_mul(std::mem::size_of::<Const>()),
        )
}

/// Same as [`run_source`], but also returns the per-function
/// [`FunctionFeedback`] counters accumulated during this one run.
pub fn run_source_with_feedback(
    source: &str,
) -> Result<(Value, Vec<FunctionFeedback>), AtomicJsError> {
    CompiledProgram::compile(source)?.run_with_feedback()
}

fn compile_module(source: &str) -> Result<BytecodeModule, AtomicJsError> {
    let tokens = crate::lexer::tokenize(source).map_err(|e| AtomicJsError(e.0))?;
    let program = crate::parser::parse(tokens).map_err(|e| AtomicJsError(e.0))?;
    crate::compiler::compile(program).map_err(|e| AtomicJsError(e.0))
}

fn execute_function<F: FeedbackSink>(
    module: &BytecodeModule,
    function_index: usize,
    args: &[Value],
    upvalues: &[Rc<RefCell<Value>>],
    feedback: &mut F,
    pools: &VmPools,
) -> Result<Value, AtomicJsError> {
    let function = &module.functions[function_index];
    feedback.on_function_call(function_index);

    let mut locals: Vec<LocalSlot> = pools.take_locals();
    for i in 0..function.local_count {
        let initial = args.get(i).cloned().unwrap_or(Value::Undefined);
        if function.captured_locals.contains(&i) {
            locals.push(LocalSlot::Captured(Rc::new(RefCell::new(initial))));
        } else {
            locals.push(LocalSlot::Plain(initial));
        }
    }

    let mut stack: Vec<Value> = pools.take_stack();
    let mut property_caches = function
        .has_property_reads
        .then(|| vec![None::<PropertyCache>; function.code.len()]);
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
            Instr::GetLocalProp { local, name } => {
                let name = property_name(function, *name);
                let cache = property_caches.as_mut().map(|entries| &mut entries[pc]);
                stack.push(cached_local_property(&locals[*local as usize], name, cache));
                pc += 1;
            }
            Instr::GetUpvalueProp { upvalue, name } => {
                let name = property_name(function, *name);
                let value = upvalues[*upvalue as usize].borrow();
                let property = match &*value {
                    Value::Object(object) => cached_property(
                        object,
                        name,
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
                // Preserve compound-assignment evaluation order: a call can
                // mutate a captured `target` local before it returns.
                let total = locals[*target as usize].number();
                let result = call_local0(module, &locals[*callee as usize], feedback, pools)?;
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
                    other => panic!(
                        "internal error: BinaryLocalConst's constant must be a Number, got {other:?}"
                    ),
                };
                stack.push(numeric_value(*op, locals[*local as usize].number(), constant));
                pc += 1;
            }
            Instr::IncrementLocal(slot) => {
                let incremented = locals[*slot as usize].number() + 1.0;
                locals[*slot as usize].set_number(incremented);
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
                // A jump whose target is at or before this instruction's own
                // position is a loop back-edge. It is recorded only when the
                // caller explicitly requested introspection feedback.
                if *target <= pc {
                    feedback.on_loop_backedge(function_index);
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
                let function_data = match callee {
                    Value::Function(f) => f,
                    other => {
                        return Err(AtomicJsError(format!(
                            "attempted to call a non-function value: {other}"
                        )))
                    }
                };
                let result = if argc == 0 {
                    // The closure benchmark calls a zero-argument function a
                    // million times. Avoid both the pooled-argument-buffer
                    // bookkeeping and cloning `captured_env`: the callee's
                    // Rc keeps its environment alive throughout this call, so
                    // a borrowed slice is sufficient.
                    execute_function(
                        module,
                        function_data.function_index,
                        &[],
                        &function_data.captured_env,
                        feedback,
                        pools,
                    )
                } else {
                    // Args were pushed left-to-right before the callee
                    // (compiler.rs's `Expr::Call` convention), so popping
                    // `argc` times yields them last-pushed-first; reverse to
                    // restore their original order. Keep the buffer pooling
                    // for non-zero-argument calls, where it avoids a real
                    // allocation.
                    let mut args = pools.take_args();
                    for _ in 0..argc {
                        args.push(stack.pop().expect("Call needs argc values on the stack"));
                    }
                    args.reverse();
                    let result = execute_function(
                        module,
                        function_data.function_index,
                        &args,
                        &function_data.captured_env,
                        feedback,
                        pools,
                    );
                    pools.return_args(args);
                    result
                }?;
                stack.push(result);
                pc += 1;
            }
            Instr::CallLocal0(local) => {
                let result = call_local0(module, &locals[*local as usize], feedback, pools)?;
                stack.push(result);
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

/// Invokes a known zero-argument local without materializing its function
/// value on the operand stack. Captured locals still use an owned fallback so
/// their RefCell borrow cannot outlive a recursive JS call.
fn call_local0<F: FeedbackSink>(
    module: &BytecodeModule,
    local: &LocalSlot,
    feedback: &mut F,
    pools: &VmPools,
) -> Result<Value, AtomicJsError> {
    match local {
        LocalSlot::Plain(Value::Function(function_data)) => execute_function(
            module,
            function_data.function_index,
            &[],
            &function_data.captured_env,
            feedback,
            pools,
        ),
        _ => match local.get() {
            Value::Function(function_data) => execute_function(
                module,
                function_data.function_index,
                &[],
                &function_data.captured_env,
                feedback,
                pools,
            ),
            other => Err(AtomicJsError(format!(
                "attempted to call a non-function value: {other}"
            ))),
        },
    }
}

fn binary_number(stack: &mut Vec<Value>, op: impl FnOnce(f64, f64) -> f64) {
    let b = stack.pop().expect("binary operation needs two operands");
    let a = stack.pop().expect("binary operation needs two operands");
    stack.push(Value::Number(op(as_number(&a), as_number(&b))));
}

fn numeric_value(op: NumericOp, left: f64, right: f64) -> Value {
    match op {
        NumericOp::Add => Value::Number(left + right),
        NumericOp::Sub => Value::Number(left - right),
        NumericOp::Mul => Value::Number(left * right),
        NumericOp::Div => Value::Number(left / right),
        NumericOp::Mod => Value::Number(left % right),
        NumericOp::Lt => Value::Bool(left < right),
    }
}

fn cached_local_property(
    local: &LocalSlot,
    name: &str,
    cache: Option<&mut Option<PropertyCache>>,
) -> Value {
    match local {
        LocalSlot::Plain(Value::Object(object)) => cached_property(object, name, cache),
        LocalSlot::Captured(cell) => match &*cell.borrow() {
            Value::Object(object) => cached_property(object, name, cache),
            _ => Value::Undefined,
        },
        _ => Value::Undefined,
    }
}

fn cached_property(
    object: &Rc<RefCell<JsObject>>,
    name: &str,
    cache: Option<&mut Option<PropertyCache>>,
) -> Value {
    let object = object.borrow();
    if let Some(cache) = cache {
        if let Some(entry) = *cache {
            if entry.shape_id == object.shape_id {
                return object.get_at(entry.slot);
            }
        }
        if let Some((slot, value)) = object.get_with_slot(name) {
            *cache = Some(PropertyCache {
                shape_id: object.shape_id,
                slot,
            });
            value
        } else {
            Value::Undefined
        }
    } else {
        object.get(name)
    }
}

fn property_name(function: &BytecodeFunction, idx: u32) -> &str {
    match &function.constants[idx as usize] {
        Const::String(name) => name,
        other => panic!("internal error: property opcode constant must be a String, got {other:?}"),
    }
}
