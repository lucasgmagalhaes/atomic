//! Bytecode ISA, constant pool, and a disassembler — see
//! spec/proposals/ATOMIC_JS_SPIKE.md §5.3.
//!
//! Two real gaps found while implementing the compiler, not in the
//! originally-drafted ISA table (same kind of honest correction as the
//! postfix/prefix `++` ambiguity caught in the lexer):
//!
//! - `LoadUpvalue`/`StoreUpvalue`: a closure needs *some* way to address a
//!   variable that isn't one of its own locals. `MakeClosure` grew a
//!   `captures` field (which of the *enclosing* frame's local slots to hand
//!   over, in the order the closure's own upvalues expect them) for the same
//!   reason.
//! - `SetProp`'s semantics changed from "pop object, mutate, done" to
//!   "mutate in place, leave the object on the stack": building a
//!   multi-property object literal needs the object reference to survive
//!   across several `SetProp` calls plus the final push as the expression's
//!   own value — the alternative was adding a `Dup` opcode, which the spike's
//!   hard scope limits (§3) explicitly exclude. None of the reference
//!   programs ever use `SetProp` outside object-literal construction, so
//!   this narrower semantics is sufficient for the spike's scope.

#[derive(Debug, Clone, PartialEq)]
pub enum Instr {
    LoadConst(u32),
    LoadLocal(u32),
    StoreLocal(u32),
    LoadUpvalue(u32),
    StoreUpvalue(u32),
    /// Pops the object, pushes `object[name]` (`name` = constant pool
    /// string at this index) — `Value::Undefined` if missing.
    GetProp(u32),
    /// Reads a property from a known local/upvalue without materializing its
    /// object value on the operand stack first.
    GetLocalProp {
        local: u32,
        name: u32,
    },
    GetUpvalueProp {
        upvalue: u32,
        name: u32,
    },
    /// Pops the value; the object below it on the stack is mutated in place
    /// (`object[name] = value`) and stays on the stack — see this module's
    /// doc for why.
    SetProp(u32),
    NewObject,
    Add,
    /// Updates a local with the sum of two local numeric values. Emitted only
    /// for a discarded `target += value` expression.
    AddLocalLocal {
        target: u32,
        value: u32,
    },
    AddLocalConst { target: u32, constant: u32 },
    /// Adds a direct local-object property to a local numeric accumulator.
    /// Emitted only for a discarded `target += object.property` expression.
    AddLocalProp {
        target: u32,
        object: u32,
        name: u32,
    },
    /// Calls a zero-argument local function and adds its result to a local
    /// numeric accumulator. Emitted only for a discarded `target += call()`.
    AddLocalCallLocal0 {
        target: u32,
        callee: u32,
    },
    /// Evaluates a numeric operation on two known local values and pushes the
    /// result, avoiding two separate local-load dispatches.
    BinaryLocalLocal {
        op: NumericOp,
        left: u32,
        right: u32,
    },
    /// Evaluates a numeric operation on a known local and numeric constant.
    BinaryLocalConst {
        op: NumericOp,
        local: u32,
        constant: u32,
    },
    /// Increments a local numeric value. Emitted only for a discarded
    /// increment expression.
    IncrementLocal(u32),
    /// Increments a captured numeric value and pushes its new value.
    IncrementUpvalue(u32),
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Jump(usize),
    JumpIfFalse(usize),
    MakeClosure {
        function_index: u32,
        /// Local slot indices in the *currently executing* frame to hand
        /// over as the new closure's upvalues, in the order the target
        /// function's own `LoadUpvalue`/`StoreUpvalue` indices expect.
        captures: Vec<u32>,
    },
    Call(u32),
    /// Calls a known local function with no arguments without first loading
    /// its `Rc<FunctionData>` onto the operand stack.
    CallLocal0(u32),
    CallNative(NativeFn),
    Return,
    Pop,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumericOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NativeFn {
    Sqrt,
    Log,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Number(f64),
    String(String),
    Undefined,
}

#[derive(Debug, Clone, Default)]
pub struct BytecodeFunction {
    pub name: Option<String>,
    pub param_count: usize,
    pub local_count: usize,
    /// Indices (within this function's own locals) that a nested function
    /// captures. The VM boxes these as `Rc<RefCell<Value>>` at frame-entry
    /// instead of storing them as plain stack slots (spec §5.4).
    pub captured_locals: Vec<usize>,
    /// Whether this function needs per-execution property inline-cache
    /// storage. Functions without direct property reads pay no allocation.
    pub has_property_reads: bool,
    pub code: Vec<Instr>,
    pub constants: Vec<Const>,
}

#[derive(Debug, Clone)]
pub struct BytecodeModule {
    pub functions: Vec<BytecodeFunction>,
    pub top_level: usize,
}

pub fn disassemble(module: &BytecodeModule) -> String {
    let mut out = String::new();
    for (i, function) in module.functions.iter().enumerate() {
        let label = function.name.as_deref().unwrap_or("<anonymous>");
        let marker = if i == module.top_level {
            " (top level)"
        } else {
            ""
        };
        out.push_str(&format!(
            "function[{i}] {label}{marker} (params={}, locals={}, captured={:?})\n",
            function.param_count, function.local_count, function.captured_locals
        ));
        for (pc, instr) in function.code.iter().enumerate() {
            out.push_str(&format!("  {pc:>4}: {instr:?}\n"));
        }
    }
    out
}
