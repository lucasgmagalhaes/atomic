//! @spec atomicjs-profiling#tier-one-expansion
//! Guarded non-scalar Tier-1 operations.

use std::cell::RefCell;
use std::rc::Rc;

use crate::value::Value;

use super::TierOneResult;

pub(super) fn property_read(
    args: &[Value],
    param_count: usize,
    local: usize,
    name: &str,
) -> Option<TierOneResult> {
    if args.len() != param_count {
        return None;
    }
    let Value::Object(object) = args.get(local)? else {
        return None;
    };
    let object = object.borrow();
    let (_, Value::Number(value)) = object.get_with_slot(name)? else {
        return None;
    };
    Some(TierOneResult {
        value: Value::Number(value),
        loop_backedges: 0,
    })
}

pub(super) fn increment_upvalue(
    upvalues: &[Rc<RefCell<Value>>],
    index: usize,
) -> Option<TierOneResult> {
    let mut value = upvalues.get(index)?.borrow_mut();
    let Value::Number(number) = &*value else {
        return None;
    };
    let incremented = *number + 1.0;
    *value = Value::Number(incremented);
    Some(TierOneResult {
        value: Value::Number(incremented),
        loop_backedges: 0,
    })
}
