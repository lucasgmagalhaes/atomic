//! @spec atomicjs-profiling#modular-execution-core
//! Expression lowering for the AtomicJS compiler.

use crate::ast::{BinOp, Expr};
use crate::bytecode::{Const, Instr, NativeFn, NumericOp};

use super::analysis::resolve;
use super::{CompileError, Compiler, Scope, SlotRef};

impl Compiler {
    pub(super) fn compile_expr(
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
                if let Expr::Identifier(name) = &**target {
                    if let SlotRef::Upvalue(upvalue) = resolve(name, fb, parent, upvalues)? {
                        fb.borrow_mut().code.push(Instr::IncrementUpvalue(upvalue));
                        return Ok(());
                    }
                }
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
                if let Expr::Identifier(name) = &**callee {
                    let local_shadows_name = fb.borrow().locals.iter().any(|local| local == name);
                    if !local_shadows_name {
                        if let Some(&function_index) = self.direct_globals.get(name) {
                            for arg in args {
                                self.compile_expr(arg, fb, parent, upvalues)?;
                            }
                            fb.borrow_mut().code.push(Instr::CallDirect {
                                function_index,
                                argc: args.len() as u32,
                            });
                            return Ok(());
                        }
                    }
                }
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
                        SlotRef::Local(local) => fb
                            .borrow_mut()
                            .code
                            .push(Instr::GetLocalProp { local, name: idx }),
                        SlotRef::Upvalue(upvalue) => fb
                            .borrow_mut()
                            .code
                            .push(Instr::GetUpvalueProp { upvalue, name: idx }),
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
}

pub(super) fn bin_instr(op: BinOp) -> Instr {
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
