//! @spec atomicjs-profiling#modular-execution-core
//! Object-property access and inline shape-cache helpers.

use std::cell::RefCell;
use std::rc::Rc;

use crate::bytecode::{BytecodeFunction, Const};
use crate::value::{JsObject, Value};

use super::state::LocalSlot;

#[derive(Clone, Copy)]
pub(super) struct PropertyCache {
    shape_id: u64,
    slot: usize,
}

pub(super) fn cached_local_property(
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

pub(super) fn cached_property(
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

pub(super) fn property_name(function: &BytecodeFunction, idx: u32) -> &str {
    match &function.constants[idx as usize] {
        Const::String(name) => name,
        other => panic!("internal error: property opcode constant must be a String, got {other:?}"),
    }
}
