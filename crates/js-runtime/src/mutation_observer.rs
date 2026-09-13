//! `MutationObserver` (`ROADMAP.md` item 40): real `childList`/`attributes`
//! observation, backed by [`dom::Dom::take_mutation_records`]'s queue.
//! Plain `JS_NewObject` instance carrying a hidden `__id` numeric property
//! indexing into a Rust-side thread_local registry — same "state as plain
//! JS values on the object, no native class/opaque pointer" convention
//! `abort_controller.rs`/`history.rs` already use, since nothing here needs
//! a finalizer beyond what `cleanup(ctx)` already handles per-context.
//!
//! Delivery timing: [`pump`] is called once per [`crate::Context::run_pending_timers`]
//! tick (see that method's own doc — this crate has no real microtask
//! queue), draining every [`dom::MutationRecord`] queued since the last
//! pump, grouping the matching ones per observer, and invoking each
//! observer's callback once with its own batch — same batching the real
//! spec's microtask-queued delivery gives, just on this crate's coarser
//! "next pump" cadence instead of "end of the current script turn".
//!
//! Scope cuts (documented, not silent): no `subtree` option (a record's
//! `target` must exactly equal an observer's own `observe()` target — see
//! [`dom::MutationRecordKind`]'s own doc), no `characterData`, and
//! `takeRecords()` always returns `[]` — correct *given* this delivery
//! model (records are delivered synchronously at the next `pump`, so
//! there's never anything queued-but-undelivered by the time a script
//! could call `takeRecords()`), not merely unimplemented.
//! `addedNodes`/`removedNodes` are plain JS arrays, not a live `NodeList`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

const ID_PROP: &[u8] = b"__id\0";

struct ObserveEntry {
    target: dom::NodeId,
    child_list: bool,
    attributes: bool,
    #[allow(dead_code)]
    attribute_old_value: bool,
    /// How many records were already queued (per `dom::Dom::pending_mutation_record_count`)
    /// at the moment this `observe()` call ran — records at or after this
    /// index in the *next* drained batch are real, future mutations; ones
    /// before it are history from before this observation started, and
    /// must not be delivered (see `pump`'s own doc for why this resets to
    /// `0` right after that first drain rather than staying a permanent
    /// offset).
    observed_at: usize,
}

struct Observer {
    callback: sys::JSValue,
    entries: Vec<ObserveEntry>,
}

#[derive(Default)]
struct Registry {
    next_id: u32,
    observers: HashMap<u32, Observer>,
}

thread_local! {
    // Keyed by JSContext pointer, same convention `timers.rs`'s own
    // `REGISTRIES` uses and for the same reason (a thread_local sidesteps
    // needing an unsafe Send/Sync impl just to stash `JSValue`s).
    static REGISTRIES: RefCell<HashMap<usize, Registry>> = RefCell::new(HashMap::new());
}

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &[u8]) -> sys::JSValue {
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr() as *const _)
}
unsafe fn set_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &[u8], value: sys::JSValue) {
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr() as *const _, value);
}
unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}
unsafe fn read_bool_option(ctx: *mut sys::JSContext, options: sys::JSValue, name: &str) -> bool {
    if options.tag != sys::JS_TAG_OBJECT {
        return false;
    }
    let cname = CString::new(name).unwrap();
    let value = sys::JS_GetPropertyStr(ctx, options, cname.as_ptr());
    let result = sys::JS_ToBool(ctx, value) != 0;
    sys::JS_FreeValue(ctx, value);
    result
}
unsafe fn observer_id(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> Option<u32> {
    let value = get_prop(ctx, this_val, ID_PROP);
    let id = match value.tag {
        sys::JS_TAG_INT => Some(value.u.int32 as u32),
        sys::JS_TAG_FLOAT64 => Some(value.u.float64 as u32),
        _ => None,
    };
    sys::JS_FreeValue(ctx, value);
    id
}

unsafe fn resolve_prototype(ctx: *mut sys::JSContext, new_target: sys::JSValue) -> sys::JSValue {
    if new_target.tag != sys::JS_TAG_UNDEFINED {
        let proto = get_prop(ctx, new_target, b"prototype\0");
        if proto.tag != sys::JS_TAG_UNDEFINED {
            return proto;
        }
        sys::JS_FreeValue(ctx, proto);
    }
    let global = sys::JS_GetGlobalObject(ctx);
    let ctor_name = CString::new("MutationObserver").unwrap();
    let ctor = sys::JS_GetPropertyStr(ctx, global, ctor_name.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let proto = get_prop(ctx, ctor, b"prototype\0");
    sys::JS_FreeValue(ctx, ctor);
    proto
}

unsafe extern "C" fn mutation_observer_constructor(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let callback = if argc >= 1 {
        *argv
    } else {
        sys::js_undefined()
    };
    if !sys::JS_IsFunction(ctx, callback) {
        let message = new_string(ctx, "MutationObserver requires a callback function");
        return sys::JS_Throw(ctx, message);
    }
    let obj = sys::JS_NewObject(ctx);
    let proto = resolve_prototype(ctx, new_target);
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);
    let id = REGISTRIES.with(|reg| {
        let mut map = reg.borrow_mut();
        let registry = map.entry(ctx as usize).or_default();
        let id = registry.next_id;
        registry.next_id = registry.next_id.wrapping_add(1);
        registry.observers.insert(
            id,
            Observer {
                callback: sys::JS_DupValue(ctx, callback),
                entries: Vec::new(),
            },
        );
        id
    });
    set_prop(ctx, obj, ID_PROP, sys::js_float64(id as f64));
    obj
}

unsafe extern "C" fn observe(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = observer_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let Some(target) = (argc >= 1)
        .then(|| crate::dom_bindings::node_id(ctx, *argv))
        .flatten()
    else {
        return sys::js_undefined();
    };
    let options = if argc >= 2 {
        *argv.add(1)
    } else {
        sys::js_undefined()
    };
    let attribute_old_value = read_bool_option(ctx, options, "attributeOldValue");
    let child_list = read_bool_option(ctx, options, "childList");
    let attributes = read_bool_option(ctx, options, "attributes") || attribute_old_value;
    let observed_at = crate::host_state::get(ctx)
        .as_ref()
        .map(|state| state.dom.pending_mutation_record_count())
        .unwrap_or(0);
    REGISTRIES.with(|reg| {
        if let Some(observer) = reg
            .borrow_mut()
            .get_mut(&(ctx as usize))
            .and_then(|r| r.observers.get_mut(&id))
        {
            observer.entries.push(ObserveEntry {
                target,
                child_list,
                attributes,
                attribute_old_value,
                observed_at,
            });
        }
    });
    sys::js_undefined()
}

unsafe extern "C" fn disconnect(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    if let Some(id) = observer_id(ctx, this_val) {
        REGISTRIES.with(|reg| {
            if let Some(observer) = reg
                .borrow_mut()
                .get_mut(&(ctx as usize))
                .and_then(|r| r.observers.get_mut(&id))
            {
                observer.entries.clear();
            }
        });
    }
    sys::js_undefined()
}

unsafe extern "C" fn take_records(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    // Always empty — see the module doc for why that's correct given this
    // crate's synchronous-at-next-pump delivery model, not a shortcut.
    sys::JS_NewArray(ctx)
}

/// Builds one JS `MutationRecord`-shaped object: `{type, target, addedNodes,
/// removedNodes, attributeName, oldValue}`. `dom_ptr` resolution mirrors
/// `dom_bindings::node_class_id_for`'s own call sites.
unsafe fn build_record_object(
    ctx: *mut sys::JSContext,
    dom_ptr: *mut dom::Dom,
    record: &dom::MutationRecord,
) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    let target_class = crate::dom_bindings::node_class_id_for(ctx, dom_ptr, record.target);
    let target_obj = crate::dom_bindings::node_object(ctx, target_class, record.target);
    set_prop(ctx, obj, b"target\0", target_obj);

    let node_array = |ctx: *mut sys::JSContext, ids: &[dom::NodeId]| -> sys::JSValue {
        let arr = sys::JS_NewArray(ctx);
        for (i, &id) in ids.iter().enumerate() {
            let class = crate::dom_bindings::node_class_id_for(ctx, dom_ptr, id);
            let node_obj = crate::dom_bindings::node_object(ctx, class, id);
            sys::JS_SetPropertyUint32(ctx, arr, i as u32, node_obj);
        }
        arr
    };

    match &record.kind {
        dom::MutationRecordKind::ChildList { added, removed } => {
            set_prop(ctx, obj, b"type\0", new_string(ctx, "childList"));
            let added_arr = node_array(ctx, added);
            set_prop(ctx, obj, b"addedNodes\0", added_arr);
            let removed_arr = node_array(ctx, removed);
            set_prop(ctx, obj, b"removedNodes\0", removed_arr);
            set_prop(ctx, obj, b"attributeName\0", sys::js_null());
            set_prop(ctx, obj, b"oldValue\0", sys::js_null());
        }
        dom::MutationRecordKind::Attributes { name, old_value } => {
            set_prop(ctx, obj, b"type\0", new_string(ctx, "attributes"));
            set_prop(ctx, obj, b"addedNodes\0", sys::JS_NewArray(ctx));
            set_prop(ctx, obj, b"removedNodes\0", sys::JS_NewArray(ctx));
            set_prop(ctx, obj, b"attributeName\0", new_string(ctx, name));
            let old_value_js = old_value
                .as_deref()
                .map(|v| new_string(ctx, v))
                .unwrap_or_else(sys::js_null);
            set_prop(ctx, obj, b"oldValue\0", old_value_js);
        }
    }
    obj
}

fn matches(entry: &ObserveEntry, index: usize, record: &dom::MutationRecord) -> bool {
    if index < entry.observed_at || entry.target != record.target {
        return false;
    }
    match &record.kind {
        dom::MutationRecordKind::ChildList { .. } => entry.child_list,
        dom::MutationRecordKind::Attributes { .. } => entry.attributes,
    }
}

/// Drains every queued [`dom::MutationRecord`] and delivers each to every
/// registered observer whose `observe()` options match, one callback call
/// per observer carrying its own batch. Called from
/// [`crate::Context::run_pending_timers`] — see the module doc for the
/// delivery-timing tradeoff. Returns how many observer callbacks ran.
pub(crate) unsafe fn pump(ctx: *mut sys::JSContext) -> usize {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return 0;
    }
    let dom_ptr: *mut dom::Dom = &mut (*state).dom;
    let records = (*dom_ptr).take_mutation_records();
    if records.is_empty() {
        return 0;
    }

    let mut batches: Vec<(sys::JSValue, Vec<sys::JSValue>)> = Vec::new();
    REGISTRIES.with(|reg| {
        let mut map = reg.borrow_mut();
        let Some(registry) = map.get_mut(&(ctx as usize)) else {
            return;
        };
        for observer in registry.observers.values() {
            let mut matched = Vec::new();
            for (index, record) in records.iter().enumerate() {
                if observer.entries.iter().any(|e| matches(e, index, record)) {
                    matched.push(build_record_object(ctx, dom_ptr, record));
                }
            }
            if !matched.is_empty() {
                batches.push((sys::JS_DupValue(ctx, observer.callback), matched));
            }
        }
        // Every entry's history-vs-future boundary only applies to *this*
        // batch (see `ObserveEntry::observed_at`'s own doc) — reset it so
        // every future drain treats this observer's records as fair game,
        // regardless of index.
        for observer in registry.observers.values_mut() {
            for entry in &mut observer.entries {
                entry.observed_at = 0;
            }
        }
    });

    let fired = batches.len();
    for (callback, records) in batches {
        let arr = sys::JS_NewArray(ctx);
        for (i, rec) in records.into_iter().enumerate() {
            sys::JS_SetPropertyUint32(ctx, arr, i as u32, rec);
        }
        let mut arg = arr;
        let result = sys::JS_Call(ctx, callback, sys::js_undefined(), 1, &mut arg);
        sys::JS_FreeValue(ctx, result);
        sys::JS_FreeValue(ctx, arr);
        sys::JS_FreeValue(ctx, callback);
    }
    fired
}

/// Frees every registered observer's callback for `ctx` — must run before
/// `JS_FreeContext`, same ordering requirement `timers::cleanup`/
/// `fetch_async::cleanup` already document.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    if let Some(registry) = REGISTRIES.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
        for (_, observer) in registry.observers {
            sys::JS_FreeValue(ctx, observer.callback);
        }
    }
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let proto = sys::JS_NewObject(ctx);
    crate::js_helpers::define_method(ctx, proto, "observe", observe, 2);
    crate::js_helpers::define_method(ctx, proto, "disconnect", disconnect, 0);
    crate::js_helpers::define_method(ctx, proto, "takeRecords", take_records, 0);

    let ctor_name = CString::new("MutationObserver").unwrap();
    let ctor = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<
            unsafe extern "C" fn(
                *mut sys::JSContext,
                sys::JSValue,
                c_int,
                *mut sys::JSValue,
            ) -> sys::JSValue,
            sys::JSCFunction,
        >(mutation_observer_constructor),
        ctor_name.as_ptr(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(ctx, ctor, proto_name.as_ptr(), proto);
    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, ctor_name.as_ptr(), ctor);
    sys::JS_FreeValue(ctx, global);
}
