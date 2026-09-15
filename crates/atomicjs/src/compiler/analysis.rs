//! @spec atomicjs-profiling#conditional-else
//! Static name analysis used before lowering bytecode.

use std::collections::HashSet;

use super::*;

pub(super) fn resolve(
    name: &str,
    fb: &Scope,
    parent: Option<&Scope>,
    upvalues: &mut Vec<u32>,
) -> Result<SlotRef, CompileError> {
    if let Some(pos) = fb
        .borrow()
        .locals
        .iter()
        .position(|candidate| candidate == name)
    {
        return Ok(SlotRef::Local(pos as u32));
    }
    if let Some(parent) = parent {
        let parent_pos = parent
            .borrow()
            .locals
            .iter()
            .position(|candidate| candidate == name);
        if let Some(parent_pos) = parent_pos {
            if let Some(existing) = upvalues.iter().position(|&slot| slot == parent_pos as u32) {
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

pub(super) fn collect_locals(body: &[Stmt], locals: &mut Vec<String>) {
    for statement in body {
        collect_locals_stmt(statement, locals);
    }
}

fn collect_locals_stmt(statement: &Stmt, locals: &mut Vec<String>) {
    match statement {
        Stmt::Let { name, .. } | Stmt::Const { name, .. } => add_local(name, locals),
        Stmt::Function(declaration) => add_local(
            declaration
                .name
                .as_ref()
                .expect("function declarations are named"),
            locals,
        ),
        Stmt::For { init, body, .. } => {
            collect_locals_stmt(init, locals);
            for statement in body {
                collect_locals_stmt(statement, locals);
            }
        }
        Stmt::While { body, .. } | Stmt::Block(body) => {
            for statement in body {
                collect_locals_stmt(statement, locals);
            }
        }
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            for statement in then_branch.iter().chain(else_branch) {
                collect_locals_stmt(statement, locals);
            }
        }
        Stmt::Expr(_) | Stmt::Return(_) => {}
    }
}

fn add_local(name: &str, locals: &mut Vec<String>) {
    if !locals.iter().any(|local| local == name) {
        locals.push(name.to_owned());
    }
}

pub(super) fn assigned_names(program: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    for statement in program {
        collect_assigned_stmt(statement, &mut names);
    }
    names
}

fn collect_assigned_stmt(statement: &Stmt, names: &mut HashSet<String>) {
    match statement {
        Stmt::Expr(expression)
        | Stmt::Let {
            value: expression, ..
        }
        | Stmt::Const {
            value: expression, ..
        } => collect_assigned_expr(expression, names),
        Stmt::Function(declaration) => {
            for statement in &declaration.body {
                collect_assigned_stmt(statement, names);
            }
        }
        Stmt::For {
            init,
            cond,
            update,
            body,
        } => {
            collect_assigned_stmt(init, names);
            collect_assigned_expr(cond, names);
            collect_assigned_expr(update, names);
            for statement in body {
                collect_assigned_stmt(statement, names);
            }
        }
        Stmt::While { cond, body } => {
            collect_assigned_expr(cond, names);
            for statement in body {
                collect_assigned_stmt(statement, names);
            }
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_assigned_expr(cond, names);
            for statement in then_branch.iter().chain(else_branch) {
                collect_assigned_stmt(statement, names);
            }
        }
        Stmt::Return(Some(expression)) => collect_assigned_expr(expression, names),
        Stmt::Return(None) => {}
        Stmt::Block(statements) => {
            for statement in statements {
                collect_assigned_stmt(statement, names);
            }
        }
    }
}

fn collect_assigned_expr(expression: &Expr, names: &mut HashSet<String>) {
    match expression {
        Expr::Assign { target, value } | Expr::CompoundAssign { target, value, .. } => {
            record_target(target, names);
            collect_assigned_expr(target, names);
            collect_assigned_expr(value, names);
        }
        Expr::Increment { target, .. } => {
            record_target(target, names);
            collect_assigned_expr(target, names);
        }
        Expr::Binary { left, right, .. } => {
            collect_assigned_expr(left, names);
            collect_assigned_expr(right, names);
        }
        Expr::Call { callee, args } => {
            collect_assigned_expr(callee, names);
            for argument in args {
                collect_assigned_expr(argument, names);
            }
        }
        Expr::Member { object, .. } => collect_assigned_expr(object, names),
        Expr::ObjectLiteral(properties) => {
            for (_, value) in properties {
                collect_assigned_expr(value, names);
            }
        }
        Expr::FunctionExpr(declaration) => {
            for statement in &declaration.body {
                collect_assigned_stmt(statement, names);
            }
        }
        Expr::Number(_) | Expr::Identifier(_) => {}
    }
}

fn record_target(target: &Expr, names: &mut HashSet<String>) {
    if let Expr::Identifier(name) = target {
        names.insert(name.clone());
    }
}
