//! @spec atomicjs-profiling#conditional-else
//! AST -> bytecode compiler, including the captured-variable pass for
//! closures — see spec/proposals/ATOMIC_JS_SPIKE.md §5.3/§5.4. Kept as a
//! module here rather than a separate crate: the rejected proposal split
//! `compiler` out specifically to let multiple codegen backends (baseline
//! JIT, mid-tier JIT) share one lowering pipeline, and that justification
//! doesn't exist with no JIT in this spike's scope (§3).
//!
//! Upvalue resolution is deliberately one level deep only: a nested
//! function resolves names against its *immediate* enclosing function's
//! locals, not further up. None of the five reference programs nest a
//! closure inside a closure inside a closure, so a grandparent-referencing
//! closure isn't supported — a documented scope cut, not an oversight.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{BinOp, Expr, Stmt};
use crate::bytecode::{BytecodeFunction, BytecodeModule, Const, Instr};

mod analysis;
mod expressions;
mod statements;

use analysis::{assigned_names, collect_locals, resolve};
use expressions::bin_instr;

#[derive(Debug, PartialEq)]
pub struct CompileError(pub String);

pub fn compile(program: Vec<Stmt>) -> Result<BytecodeModule, CompileError> {
    let mut compiler = Compiler {
        functions: Vec::new(),
        direct_globals: HashMap::new(),
        reassigned_names: assigned_names(&program),
    };
    let (top_index, captures) = compiler.compile_function(None, &[], &program, None, true)?;
    debug_assert!(
        captures.is_empty(),
        "the top-level frame cannot itself be a closure"
    );
    Ok(BytecodeModule {
        functions: compiler.functions,
        top_level: top_index as usize,
    })
}

pub(super) struct FunctionBuilder {
    pub(super) locals: Vec<String>,
    pub(super) code: Vec<Instr>,
    pub(super) constants: Vec<Const>,
    /// Indices into `locals` that some nested function captures.
    pub(super) captured: HashSet<usize>,
}

impl FunctionBuilder {
    fn push_const(&mut self, c: Const) -> u32 {
        self.constants.push(c);
        (self.constants.len() - 1) as u32
    }
}

/// Shared, interior-mutable handle so a nested function's compilation can
/// resolve names against (and mark captures on) its parent's builder without
/// fighting the borrow checker over a mutable-reference chain — compiler-
/// internal bookkeeping only, not a runtime concern (spike scope, §3: no
/// performance pressure here).
pub(super) type Scope = Rc<RefCell<FunctionBuilder>>;

pub(super) enum SlotRef {
    Local(u32),
    Upvalue(u32),
}

pub(super) struct Compiler {
    pub(super) functions: Vec<BytecodeFunction>,
    pub(super) direct_globals: HashMap<String, u32>,
    /// Conservative semantic guard: a declaration whose name is assigned
    /// anywhere remains a dynamic closure lookup, even before Tier 1.
    pub(super) reassigned_names: HashSet<String>,
}

impl Compiler {
    /// Compiles one function (or the top-level script, when `parent` is
    /// `None` and `is_top_level` is true) and appends it to `self.functions`.
    /// Returns its index plus the `captures` list its *own* `MakeClosure`
    /// site (in the parent, if any) needs.
    pub(super) fn compile_function(
        &mut self,
        name: Option<String>,
        params: &[String],
        body: &[Stmt],
        parent: Option<&Scope>,
        is_top_level: bool,
    ) -> Result<(u32, Vec<u32>), CompileError> {
        let mut locals = params.to_vec();
        collect_locals(body, &mut locals);

        let fb: Scope = Rc::new(RefCell::new(FunctionBuilder {
            locals,
            code: Vec::new(),
            constants: Vec::new(),
            captured: HashSet::new(),
        }));

        let mut upvalues: Vec<u32> = Vec::new();
        self.compile_block(&fb, parent, body, is_top_level, &mut upvalues)?;

        // Trailing implicit `return undefined` — unreachable if every path
        // already hit an explicit Return (or the top level's own completion
        // Return, see compile_block), harmless dead code otherwise.
        let undef_idx = fb.borrow_mut().push_const(Const::Undefined);
        fb.borrow_mut().code.push(Instr::LoadConst(undef_idx));
        fb.borrow_mut().code.push(Instr::Return);

        let inner = Rc::try_unwrap(fb)
            .unwrap_or_else(|_| {
                panic!("no other reference to this function's builder should remain")
            })
            .into_inner();
        let function = BytecodeFunction {
            name,
            param_count: params.len(),
            local_count: inner.locals.len(),
            captured_locals: inner.captured.into_iter().collect(),
            upvalue_count: upvalues.len(),
            has_property_reads: inner.code.iter().any(|instruction| {
                matches!(
                    instruction,
                    Instr::GetLocalProp { .. }
                        | Instr::GetUpvalueProp { .. }
                        | Instr::AddLocalProp { .. }
                )
            }),
            code: inner.code,
            constants: inner.constants,
        };
        let index = self.functions.len() as u32;
        self.functions.push(function);

        Ok((index, upvalues))
    }

    fn compile_block(
        &mut self,
        fb: &Scope,
        parent: Option<&Scope>,
        stmts: &[Stmt],
        is_top_level: bool,
        upvalues: &mut Vec<u32>,
    ) -> Result<(), CompileError> {
        for (i, stmt) in stmts.iter().enumerate() {
            let is_last = i + 1 == stmts.len();
            if is_top_level && is_last {
                if let Stmt::Expr(expr) = stmt {
                    // Completion value (§5.4): the last top-level expression
                    // statement's value, left on the stack instead of
                    // popped, then returned.
                    self.compile_expr(expr, fb, parent, upvalues)?;
                    fb.borrow_mut().code.push(Instr::Return);
                    continue;
                }
            }
            self.compile_stmt(stmt, fb, parent, upvalues, is_top_level)?;
        }
        Ok(())
    }

    /// Compile an expression whose value is provably discarded. Assignments
    /// and increments otherwise reload the stored value only for the caller
    /// to immediately `Pop` it; avoiding that pair removes two dispatches
    /// per loop iteration in the spike's reference programs.
    pub(super) fn compile_discard_expr(
        &mut self,
        expr: &Expr,
        fb: &Scope,
        parent: Option<&Scope>,
        upvalues: &mut Vec<u32>,
    ) -> Result<(), CompileError> {
        match expr {
            Expr::Assign { target, value } => {
                self.compile_expr(value, fb, parent, upvalues)?;
                self.store(target, fb, parent, upvalues)?;
            }
            Expr::CompoundAssign { op, target, value } => {
                if *op == BinOp::Add {
                    if let (Expr::Identifier(target), Expr::Number(value)) = (&**target, &**value) {
                        if let SlotRef::Local(target) = resolve(target, fb, parent, upvalues)? {
                            let constant = fb.borrow_mut().push_const(Const::Number(*value));
                            fb.borrow_mut()
                                .code
                                .push(Instr::AddLocalConst { target, constant });
                            return Ok(());
                        }
                    }
                    if let (Expr::Identifier(target), Expr::Identifier(value)) =
                        (&**target, &**value)
                    {
                        if let (SlotRef::Local(target), SlotRef::Local(value)) = (
                            resolve(target, fb, parent, upvalues)?,
                            resolve(value, fb, parent, upvalues)?,
                        ) {
                            fb.borrow_mut()
                                .code
                                .push(Instr::AddLocalLocal { target, value });
                            return Ok(());
                        }
                    }
                    if let (Expr::Identifier(target), Expr::Member { object, property }) =
                        (&**target, &**value)
                    {
                        if let Expr::Identifier(object) = &**object {
                            if let (SlotRef::Local(target), SlotRef::Local(object)) = (
                                resolve(target, fb, parent, upvalues)?,
                                resolve(object, fb, parent, upvalues)?,
                            ) {
                                let name =
                                    fb.borrow_mut().push_const(Const::String(property.clone()));
                                fb.borrow_mut().code.push(Instr::AddLocalProp {
                                    target,
                                    object,
                                    name,
                                });
                                return Ok(());
                            }
                        }
                    }
                    if let (Expr::Identifier(target), Expr::Call { callee, args }) =
                        (&**target, &**value)
                    {
                        if args.is_empty() {
                            if let Expr::Identifier(callee) = &**callee {
                                if let (SlotRef::Local(target), SlotRef::Local(callee)) = (
                                    resolve(target, fb, parent, upvalues)?,
                                    resolve(callee, fb, parent, upvalues)?,
                                ) {
                                    fb.borrow_mut()
                                        .code
                                        .push(Instr::AddLocalCallLocal0 { target, callee });
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                self.compile_expr(target, fb, parent, upvalues)?;
                self.compile_expr(value, fb, parent, upvalues)?;
                fb.borrow_mut().code.push(bin_instr(*op));
                self.store(target, fb, parent, upvalues)?;
            }
            Expr::Increment { target, .. } => {
                if let Expr::Identifier(name) = &**target {
                    if let SlotRef::Local(local) = resolve(name, fb, parent, upvalues)? {
                        fb.borrow_mut().code.push(Instr::IncrementLocal(local));
                        return Ok(());
                    }
                }
                self.compile_expr(target, fb, parent, upvalues)?;
                let one_idx = fb.borrow_mut().push_const(Const::Number(1.0));
                fb.borrow_mut().code.push(Instr::LoadConst(one_idx));
                fb.borrow_mut().code.push(Instr::Add);
                self.store(target, fb, parent, upvalues)?;
            }
            _ => {
                self.compile_expr(expr, fb, parent, upvalues)?;
                fb.borrow_mut().code.push(Instr::Pop);
            }
        }
        Ok(())
    }

    /// Assignment/increment targets are always a plain identifier in this
    /// spike's scope (checked against all five reference programs +
    /// variants — none assign through a member expression). Stores the
    /// value already on top of the stack into `target`, then reloads it so
    /// the store also has a well-defined expression value (used by
    /// `Assign`/`CompoundAssign`/`Increment` alike).
    fn store_and_reload(
        &mut self,
        target: &Expr,
        fb: &Scope,
        parent: Option<&Scope>,
        upvalues: &mut Vec<u32>,
    ) -> Result<(), CompileError> {
        self.store(target, fb, parent, upvalues)?;
        self.compile_expr(target, fb, parent, upvalues)
    }

    fn store(
        &mut self,
        target: &Expr,
        fb: &Scope,
        parent: Option<&Scope>,
        upvalues: &mut Vec<u32>,
    ) -> Result<(), CompileError> {
        let name = match target {
            Expr::Identifier(name) => name,
            other => {
                return Err(CompileError(format!(
                    "assignment/increment target must be an identifier in this spike's scope, got {other:?}"
                )))
            }
        };
        match resolve(name, fb, parent, upvalues)? {
            SlotRef::Local(i) => {
                fb.borrow_mut().code.push(Instr::StoreLocal(i));
            }
            SlotRef::Upvalue(i) => {
                fb.borrow_mut().code.push(Instr::StoreUpvalue(i));
            }
        }
        Ok(())
    }
}

pub(super) fn local_slot(fb: &Scope, name: &str) -> u32 {
    fb.borrow()
        .locals
        .iter()
        .position(|n| n == name)
        .unwrap_or_else(|| panic!("{name} should have been collected into locals up front"))
        as u32
}
