//! @spec atomicjs-profiling#conditional-else
//! Bytecode lowering for statements and control-flow patching.

use super::*;

impl Compiler {
    pub(super) fn compile_stmt(
        &mut self,
        stmt: &Stmt,
        fb: &Scope,
        parent: Option<&Scope>,
        upvalues: &mut Vec<u32>,
        is_top_level: bool,
    ) -> Result<(), CompileError> {
        match stmt {
            Stmt::Expr(expr) => self.compile_discard_expr(expr, fb, parent, upvalues)?,
            Stmt::Let { name, value } | Stmt::Const { name, value } => {
                self.compile_expr(value, fb, parent, upvalues)?;
                let slot = local_slot(fb, name);
                fb.borrow_mut().code.push(Instr::StoreLocal(slot));
            }
            Stmt::Function(decl) => self.compile_function_decl(decl, fb, is_top_level)?,
            Stmt::For {
                init,
                cond,
                update,
                body,
            } => self.compile_for((init, cond, update, body), fb, parent, upvalues)?,
            Stmt::While { cond, body } => self.compile_while(cond, body, fb, parent, upvalues)?,
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => self.compile_if(cond, then_branch, else_branch, fb, parent, upvalues)?,
            Stmt::Return(value) => {
                if let Some(expr) = value {
                    self.compile_expr(expr, fb, parent, upvalues)?;
                } else {
                    let idx = fb.borrow_mut().push_const(Const::Undefined);
                    fb.borrow_mut().code.push(Instr::LoadConst(idx));
                }
                fb.borrow_mut().code.push(Instr::Return);
            }
            Stmt::Block(statements) => {
                for statement in statements {
                    self.compile_stmt(statement, fb, parent, upvalues, false)?;
                }
            }
        }
        Ok(())
    }

    fn compile_function_decl(
        &mut self,
        decl: &crate::ast::FunctionDecl,
        fb: &Scope,
        is_top_level: bool,
    ) -> Result<(), CompileError> {
        let name = decl.name.clone().expect("function declarations are named");
        let (function_index, captures) = self.compile_function(
            Some(name.clone()),
            &decl.params,
            &decl.body,
            Some(fb),
            false,
        )?;
        fb.borrow_mut().code.push(Instr::MakeClosure {
            function_index,
            captures,
        });
        let slot = local_slot(fb, &name);
        fb.borrow_mut().code.push(Instr::StoreLocal(slot));
        if is_top_level
            && self.functions[function_index as usize].upvalue_count == 0
            && !self.reassigned_names.contains(&name)
        {
            self.direct_globals.insert(name, function_index);
        }
        Ok(())
    }

    fn compile_for(
        &mut self,
        (init, cond, update, body): (&Stmt, &Expr, &Expr, &[Stmt]),
        fb: &Scope,
        parent: Option<&Scope>,
        upvalues: &mut Vec<u32>,
    ) -> Result<(), CompileError> {
        self.compile_stmt(init, fb, parent, upvalues, false)?;
        let start = fb.borrow().code.len();
        self.compile_expr(cond, fb, parent, upvalues)?;
        let exit = push_false_jump(fb);
        for statement in body {
            self.compile_stmt(statement, fb, parent, upvalues, false)?;
        }
        self.compile_discard_expr(update, fb, parent, upvalues)?;
        close_loop(fb, start, exit);
        Ok(())
    }

    fn compile_while(
        &mut self,
        cond: &Expr,
        body: &[Stmt],
        fb: &Scope,
        parent: Option<&Scope>,
        upvalues: &mut Vec<u32>,
    ) -> Result<(), CompileError> {
        let start = fb.borrow().code.len();
        self.compile_expr(cond, fb, parent, upvalues)?;
        let exit = push_false_jump(fb);
        for statement in body {
            self.compile_stmt(statement, fb, parent, upvalues, false)?;
        }
        close_loop(fb, start, exit);
        Ok(())
    }

    fn compile_if(
        &mut self,
        cond: &Expr,
        then_branch: &[Stmt],
        else_branch: &[Stmt],
        fb: &Scope,
        parent: Option<&Scope>,
        upvalues: &mut Vec<u32>,
    ) -> Result<(), CompileError> {
        self.compile_expr(cond, fb, parent, upvalues)?;
        let false_jump = push_false_jump(fb);
        for statement in then_branch {
            self.compile_stmt(statement, fb, parent, upvalues, false)?;
        }
        if else_branch.is_empty() {
            let end = fb.borrow().code.len();
            patch_false_jump(fb, false_jump, end);
            return Ok(());
        }
        let end_jump = fb.borrow().code.len();
        fb.borrow_mut().code.push(Instr::Jump(usize::MAX));
        let else_start = fb.borrow().code.len();
        patch_false_jump(fb, false_jump, else_start);
        for statement in else_branch {
            self.compile_stmt(statement, fb, parent, upvalues, false)?;
        }
        let end = fb.borrow().code.len();
        match &mut fb.borrow_mut().code[end_jump] {
            Instr::Jump(target) => *target = end,
            _ => unreachable!(),
        }
        Ok(())
    }
}

fn push_false_jump(fb: &Scope) -> usize {
    let index = fb.borrow().code.len();
    fb.borrow_mut().code.push(Instr::JumpIfFalse(usize::MAX));
    index
}
fn patch_false_jump(fb: &Scope, index: usize, target: usize) {
    match &mut fb.borrow_mut().code[index] {
        Instr::JumpIfFalse(slot) => *slot = target,
        _ => unreachable!(),
    }
}
fn close_loop(fb: &Scope, start: usize, exit: usize) {
    fb.borrow_mut().code.push(Instr::Jump(start));
    let end = fb.borrow().code.len();
    patch_false_jump(fb, exit, end);
}
