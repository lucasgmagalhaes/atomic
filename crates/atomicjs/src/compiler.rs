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
use std::collections::HashSet;
use std::rc::Rc;

use crate::ast::{BinOp, Expr, Stmt};
use crate::bytecode::{BytecodeFunction, BytecodeModule, Const, Instr, NativeFn, NumericOp};

#[derive(Debug, PartialEq)]
pub struct CompileError(pub String);

pub fn compile(program: Vec<Stmt>) -> Result<BytecodeModule, CompileError> {
    let mut compiler = Compiler {
        functions: Vec::new(),
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

struct FunctionBuilder {
    locals: Vec<String>,
    code: Vec<Instr>,
    constants: Vec<Const>,
    /// Indices into `locals` that some nested function captures.
    captured: HashSet<usize>,
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
type Scope = Rc<RefCell<FunctionBuilder>>;

enum SlotRef {
    Local(u32),
    Upvalue(u32),
}

/// Resolves `name` against `fb`'s own locals first, then (one level only)
/// against `parent`'s locals — registering a new upvalue and marking the
/// parent's slot captured on first reference. `upvalues` is this function's
/// own accumulating list of parent-slot-indices, one per upvalue it uses (in
/// first-reference order) — exactly the `captures` list `MakeClosure` needs
/// once this function itself is done compiling.
fn resolve(
    name: &str,
    fb: &Scope,
    parent: Option<&Scope>,
    upvalues: &mut Vec<u32>,
) -> Result<SlotRef, CompileError> {
    if let Some(pos) = fb.borrow().locals.iter().position(|n| n == name) {
        return Ok(SlotRef::Local(pos as u32));
    }
    if let Some(parent) = parent {
        // Bound to an owned `Option<usize>` first, not matched on directly —
        // `if let Some(x) = parent.borrow().foo` would keep that `Ref` alive
        // for the whole `if let` body (temporary lifetime extension), and
        // the `parent.borrow_mut()` a few lines down would then panic with
        // "already borrowed". A real bug caught by
        // `compiler_test.rs`'s closure-capture test, not a hypothetical.
        let parent_pos = parent.borrow().locals.iter().position(|n| n == name);
        if let Some(parent_pos) = parent_pos {
            if let Some(existing) = upvalues.iter().position(|&s| s == parent_pos as u32) {
                return Ok(SlotRef::Upvalue(existing as u32));
            }
            let upvalue_index = upvalues.len() as u32;
            upvalues.push(parent_pos as u32);
            parent.borrow_mut().captured.insert(parent_pos);
            return Ok(SlotRef::Upvalue(upvalue_index));
        }
    }
    Err(CompileError(format!("undefined variable: {name}")))
}

/// Collects this function's *own* local names (params first, then every
/// `let`/`const`/named-function-declaration found directly in its body) —
/// nested function bodies are excluded, they get their own locals list when
/// `compile_function` recurses into them. No block scoping: a `for` loop's
/// init and body flatten into the enclosing function's locals, same as any
/// other `let` (documented in §5.4 — none of the reference programs shadow a
/// name in a way this would break).
fn collect_locals(body: &[Stmt], locals: &mut Vec<String>) {
    for stmt in body {
        collect_locals_stmt(stmt, locals);
    }
}

fn collect_locals_stmt(stmt: &Stmt, locals: &mut Vec<String>) {
    match stmt {
        Stmt::Let { name, .. } | Stmt::Const { name, .. } => {
            if !locals.contains(name) {
                locals.push(name.clone());
            }
        }
        Stmt::Function(decl) => {
            let name = decl
                .name
                .as_ref()
                .expect("statement-level function declarations are always named");
            if !locals.contains(name) {
                locals.push(name.clone());
            }
        }
        Stmt::For { init, body, .. } => {
            collect_locals_stmt(init, locals);
            for s in body {
                collect_locals_stmt(s, locals);
            }
        }
        Stmt::If { then_branch, .. } => {
            for s in then_branch {
                collect_locals_stmt(s, locals);
            }
        }
        Stmt::Block(stmts) => {
            for s in stmts {
                collect_locals_stmt(s, locals);
            }
        }
        Stmt::Expr(_) | Stmt::Return(_) => {}
    }
}

struct Compiler {
    functions: Vec<BytecodeFunction>,
}

impl Compiler {
    /// Compiles one function (or the top-level script, when `parent` is
    /// `None` and `is_top_level` is true) and appends it to `self.functions`.
    /// Returns its index plus the `captures` list its *own* `MakeClosure`
    /// site (in the parent, if any) needs.
    fn compile_function(
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
            self.compile_stmt(stmt, fb, parent, upvalues)?;
        }
        Ok(())
    }

    fn compile_stmt(
        &mut self,
        stmt: &Stmt,
        fb: &Scope,
        parent: Option<&Scope>,
        upvalues: &mut Vec<u32>,
    ) -> Result<(), CompileError> {
        match stmt {
            Stmt::Expr(expr) => {
                self.compile_discard_expr(expr, fb, parent, upvalues)?;
            }
            Stmt::Let { name, value } | Stmt::Const { name, value } => {
                self.compile_expr(value, fb, parent, upvalues)?;
                let slot = local_slot(fb, name);
                fb.borrow_mut().code.push(Instr::StoreLocal(slot));
            }
            Stmt::Function(decl) => {
                let decl_name = decl
                    .name
                    .clone()
                    .expect("statement-level function declarations are always named");
                let (function_index, captures) = self.compile_function(
                    Some(decl_name.clone()),
                    &decl.params,
                    &decl.body,
                    Some(fb),
                    false,
                )?;
                fb.borrow_mut().code.push(Instr::MakeClosure {
                    function_index,
                    captures,
                });
                let slot = local_slot(fb, &decl_name);
                fb.borrow_mut().code.push(Instr::StoreLocal(slot));
            }
            Stmt::For {
                init,
                cond,
                update,
                body,
            } => {
                self.compile_stmt(init, fb, parent, upvalues)?;
                let loop_start = fb.borrow().code.len();
                self.compile_expr(cond, fb, parent, upvalues)?;
                let jump_if_false_idx = fb.borrow().code.len();
                fb.borrow_mut().code.push(Instr::JumpIfFalse(usize::MAX)); // patched below
                for s in body {
                    self.compile_stmt(s, fb, parent, upvalues)?;
                }
                self.compile_discard_expr(update, fb, parent, upvalues)?;
                fb.borrow_mut().code.push(Instr::Jump(loop_start));
                let loop_end = fb.borrow().code.len();
                match &mut fb.borrow_mut().code[jump_if_false_idx] {
                    Instr::JumpIfFalse(target) => *target = loop_end,
                    _ => unreachable!("jump_if_false_idx was recorded right after pushing it"),
                }
            }
            Stmt::If { cond, then_branch } => {
                self.compile_expr(cond, fb, parent, upvalues)?;
                let jump = fb.borrow().code.len();
                fb.borrow_mut().code.push(Instr::JumpIfFalse(usize::MAX));
                for stmt in then_branch {
                    self.compile_stmt(stmt, fb, parent, upvalues)?;
                }
                let end = fb.borrow().code.len();
                match &mut fb.borrow_mut().code[jump] {
                    Instr::JumpIfFalse(target) => *target = end,
                    _ => unreachable!(),
                }
            }
            Stmt::Return(value) => {
                match value {
                    Some(expr) => self.compile_expr(expr, fb, parent, upvalues)?,
                    None => {
                        let idx = fb.borrow_mut().push_const(Const::Undefined);
                        fb.borrow_mut().code.push(Instr::LoadConst(idx));
                    }
                }
                fb.borrow_mut().code.push(Instr::Return);
            }
            Stmt::Block(stmts) => {
                for s in stmts {
                    self.compile_stmt(s, fb, parent, upvalues)?;
                }
            }
        }
        Ok(())
    }

    fn compile_expr(
        &mut self,
        expr: &Expr,
        fb: &Scope,
        parent: Option<&Scope>,
        upvalues: &mut Vec<u32>,
    ) -> Result<(), CompileError> {
        match expr {
            Expr::Number(n) => {
                let idx = fb.borrow_mut().push_const(Const::Number(*n));
                fb.borrow_mut().code.push(Instr::LoadConst(idx));
            }
            Expr::Identifier(name) => match resolve(name, fb, parent, upvalues)? {
                SlotRef::Local(i) => fb.borrow_mut().code.push(Instr::LoadLocal(i)),
                SlotRef::Upvalue(i) => fb.borrow_mut().code.push(Instr::LoadUpvalue(i)),
            },
            Expr::Assign { target, value } => {
                self.compile_expr(value, fb, parent, upvalues)?;
                self.store_and_reload(target, fb, parent, upvalues)?;
            }
            Expr::CompoundAssign { op, target, value } => {
                self.compile_expr(target, fb, parent, upvalues)?;
                self.compile_expr(value, fb, parent, upvalues)?;
                fb.borrow_mut().code.push(bin_instr(*op));
                self.store_and_reload(target, fb, parent, upvalues)?;
            }
            Expr::Increment { target, .. } => {
                // Prefix and postfix compile identically — see ast.rs's own
                // doc on `Expr::Increment` for why that's a safe cut here.
                self.compile_expr(target, fb, parent, upvalues)?;
                let one_idx = fb.borrow_mut().push_const(Const::Number(1.0));
                fb.borrow_mut().code.push(Instr::LoadConst(one_idx));
                fb.borrow_mut().code.push(Instr::Add);
                self.store_and_reload(target, fb, parent, upvalues)?;
            }
            Expr::Binary { op, left, right } => {
                if let (Expr::Identifier(left), Expr::Identifier(right)) = (&**left, &**right) {
                    if let (SlotRef::Local(left), SlotRef::Local(right)) = (
                        resolve(left, fb, parent, upvalues)?,
                        resolve(right, fb, parent, upvalues)?,
                    ) {
                        fb.borrow_mut().code.push(Instr::BinaryLocalLocal {
                            op: numeric_op(*op),
                            left,
                            right,
                        });
                        return Ok(());
                    }
                }
                if let (Expr::Identifier(local), Expr::Number(number)) = (&**left, &**right) {
                    if let SlotRef::Local(local) = resolve(local, fb, parent, upvalues)? {
                        let constant = fb.borrow_mut().push_const(Const::Number(*number));
                        fb.borrow_mut().code.push(Instr::BinaryLocalConst {
                            op: numeric_op(*op),
                            local,
                            constant,
                        });
                        return Ok(());
                    }
                }
                self.compile_expr(left, fb, parent, upvalues)?;
                self.compile_expr(right, fb, parent, upvalues)?;
                fb.borrow_mut().code.push(bin_instr(*op));
            }
            Expr::Call { callee, args } => {
                if args.is_empty() {
                    if let Expr::Identifier(name) = &**callee {
                        if let SlotRef::Local(local) = resolve(name, fb, parent, upvalues)? {
                            fb.borrow_mut().code.push(Instr::CallLocal0(local));
                            return Ok(());
                        }
                    }
                }
                if args.len() == 1 {
                    if let Expr::Member { object, property } = &**callee {
                        if matches!(&**object, Expr::Identifier(name) if name == "Math") {
                            let native = match property.as_str() {
                                "sqrt" => Some(NativeFn::Sqrt),
                                "log" => Some(NativeFn::Log),
                                _ => None,
                            };
                            if let Some(native) = native {
                                self.compile_expr(&args[0], fb, parent, upvalues)?;
                                fb.borrow_mut().code.push(Instr::CallNative(native));
                                return Ok(());
                            }
                        }
                    }
                }
                for arg in args {
                    self.compile_expr(arg, fb, parent, upvalues)?;
                }
                self.compile_expr(callee, fb, parent, upvalues)?;
                fb.borrow_mut().code.push(Instr::Call(args.len() as u32));
            }
            Expr::Member { object, property } => {
                let idx = fb.borrow_mut().push_const(Const::String(property.clone()));
                if let Expr::Identifier(name) = &**object {
                    match resolve(name, fb, parent, upvalues)? {
                        SlotRef::Local(local) => {
                            fb.borrow_mut()
                                .code
                                .push(Instr::GetLocalProp { local, name: idx });
                        }
                        SlotRef::Upvalue(upvalue) => {
                            fb.borrow_mut()
                                .code
                                .push(Instr::GetUpvalueProp { upvalue, name: idx });
                        }
                    }
                } else {
                    self.compile_expr(object, fb, parent, upvalues)?;
                    fb.borrow_mut().code.push(Instr::GetProp(idx));
                }
            }
            Expr::ObjectLiteral(props) => {
                fb.borrow_mut().code.push(Instr::NewObject);
                for (key, value) in props {
                    self.compile_expr(value, fb, parent, upvalues)?;
                    let idx = fb.borrow_mut().push_const(Const::String(key.clone()));
                    fb.borrow_mut().code.push(Instr::SetProp(idx));
                }
            }
            Expr::FunctionExpr(decl) => {
                let (function_index, captures) = self.compile_function(
                    decl.name.clone(),
                    &decl.params,
                    &decl.body,
                    Some(fb),
                    false,
                )?;
                fb.borrow_mut().code.push(Instr::MakeClosure {
                    function_index,
                    captures,
                });
            }
        }
        Ok(())
    }

    /// Compile an expression whose value is provably discarded. Assignments
    /// and increments otherwise reload the stored value only for the caller
    /// to immediately `Pop` it; avoiding that pair removes two dispatches
    /// per loop iteration in the spike's reference programs.
    fn compile_discard_expr(
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
                                let name = fb.borrow_mut().push_const(Const::String(property.clone()));
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

fn bin_instr(op: BinOp) -> Instr {
    match op {
        BinOp::Add => Instr::Add,
        BinOp::Sub => Instr::Sub,
        BinOp::Mul => Instr::Mul,
        BinOp::Div => Instr::Div,
        BinOp::Mod => Instr::Mod,
        BinOp::Less => Instr::Lt,
    }
}

fn numeric_op(op: BinOp) -> NumericOp {
    match op {
        BinOp::Add => NumericOp::Add,
        BinOp::Sub => NumericOp::Sub,
        BinOp::Mul => NumericOp::Mul,
        BinOp::Div => NumericOp::Div,
        BinOp::Mod => NumericOp::Mod,
        BinOp::Less => NumericOp::Lt,
    }
}

fn local_slot(fb: &Scope, name: &str) -> u32 {
    fb.borrow()
        .locals
        .iter()
        .position(|n| n == name)
        .unwrap_or_else(|| panic!("{name} should have been collected into locals up front"))
        as u32
}
