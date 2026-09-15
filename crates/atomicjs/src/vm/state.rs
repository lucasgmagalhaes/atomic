//! @spec atomicjs-profiling#modular-execution-core
//! Per-frame values and reusable execution buffers.

use std::cell::RefCell;
use std::rc::Rc;

use crate::value::Value;

use super::operations::as_number;

pub(super) enum LocalSlot {
    Plain(Value),
    Captured(Rc<RefCell<Value>>),
}

impl LocalSlot {
    pub(super) fn get(&self) -> Value {
        match self {
            Self::Plain(v) => v.clone(),
            Self::Captured(cell) => cell.borrow().clone(),
        }
    }

    pub(super) fn set(&mut self, value: Value) {
        match self {
            Self::Plain(v) => *v = value,
            Self::Captured(cell) => *cell.borrow_mut() = value,
        }
    }

    #[inline(always)]
    pub(super) fn number(&self) -> f64 {
        match self {
            Self::Plain(Value::Number(number)) => *number,
            Self::Captured(cell) => as_number(&cell.borrow()),
            Self::Plain(value) => as_number(value),
        }
    }

    #[inline(always)]
    pub(super) fn set_number(&mut self, number: f64) {
        match self {
            Self::Plain(Value::Number(value)) => *value = number,
            Self::Captured(cell) => *cell.borrow_mut() = Value::Number(number),
            Self::Plain(value) => *value = Value::Number(number),
        }
    }
}

#[derive(Default)]
pub(super) struct VmPools {
    locals: RefCell<Vec<Vec<LocalSlot>>>,
    stack: RefCell<Vec<Vec<Value>>>,
    args: RefCell<Vec<Vec<Value>>>,
}

impl VmPools {
    pub(super) fn take_locals(&self) -> Vec<LocalSlot> {
        self.locals.borrow_mut().pop().unwrap_or_default()
    }

    pub(super) fn take_stack(&self) -> Vec<Value> {
        self.stack.borrow_mut().pop().unwrap_or_default()
    }

    pub(super) fn return_locals(&self, mut values: Vec<LocalSlot>) {
        values.clear();
        self.locals.borrow_mut().push(values);
    }

    pub(super) fn return_stack(&self, mut values: Vec<Value>) {
        values.clear();
        self.stack.borrow_mut().push(values);
    }

    pub(super) fn take_args(&self) -> Vec<Value> {
        self.args.borrow_mut().pop().unwrap_or_default()
    }

    pub(super) fn return_args(&self, mut values: Vec<Value>) {
        values.clear();
        self.args.borrow_mut().push(values);
    }
}
