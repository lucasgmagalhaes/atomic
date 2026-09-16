//! `AudioNode` (Oscillator/Gain/Destination share one opaque shape) —
//! split out from `web_audio/mod.rs`.

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};
use std::rc::Rc;

use quickjs_sys as sys;

use crate::js_helpers::{define_getter_setter, define_method};

use super::graph::{GraphState, NodeKind};
use super::helpers::{new_js_string, read_js_number, read_js_string};
use super::NODE_CLASS_KIND;

pub(super) struct AudioNodeData {
    pub(super) graph: Rc<RefCell<GraphState>>,
    pub(super) index: usize,
}

pub(super) unsafe fn node_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut AudioNodeData {
    sys::JS_GetOpaque(
        this_val,
        crate::class_registry::class_id_for(rt, NODE_CLASS_KIND),
    ) as *mut AudioNodeData
}

unsafe extern "C" fn node_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = node_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

pub(super) unsafe fn make_node_object(
    ctx: *mut sys::JSContext,
    graph: Rc<RefCell<GraphState>>,
    index: usize,
) -> sys::JSValue {
    let class_id = crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    sys::JS_SetOpaque(
        obj,
        Box::into_raw(Box::new(AudioNodeData { graph, index })) as *mut c_void,
    );
    obj
}

unsafe extern "C" fn node_connect(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let src = node_opaque(rt, this_val);
    if src.is_null() || argc < 1 {
        return sys::js_undefined();
    }
    let dest = node_opaque(rt, *argv);
    if dest.is_null() {
        return sys::js_undefined();
    }
    (*src)
        .graph
        .borrow_mut()
        .edges
        .push(((*src).index, (*dest).index));
    sys::js_undefined()
}

unsafe extern "C" fn node_start(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let when = if argc >= 1 {
        read_js_number(*argv).unwrap_or(0.0)
    } else {
        0.0
    };
    if let NodeKind::Oscillator { start, .. } = &mut (*ptr).graph.borrow_mut().nodes[(*ptr).index] {
        *start = Some(when);
    }
    sys::js_undefined()
}

unsafe extern "C" fn node_stop(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let when = if argc >= 1 {
        read_js_number(*argv).unwrap_or(0.0)
    } else {
        0.0
    };
    if let NodeKind::Oscillator { stop, .. } = &mut (*ptr).graph.borrow_mut().nodes[(*ptr).index] {
        *stop = Some(when);
    }
    sys::js_undefined()
}

unsafe extern "C" fn frequency_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_float64(0.0);
    }
    match &(*ptr).graph.borrow().nodes[(*ptr).index] {
        NodeKind::Oscillator { frequency, .. } => sys::js_float64(*frequency),
        _ => sys::js_float64(0.0),
    }
}

unsafe extern "C" fn frequency_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let Some(freq) = read_js_number(val) else {
        return sys::js_undefined();
    };
    if !ptr.is_null() {
        if let NodeKind::Oscillator { frequency, .. } =
            &mut (*ptr).graph.borrow_mut().nodes[(*ptr).index]
        {
            *frequency = freq;
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn wave_type_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return new_js_string(ctx, "sine");
    }
    match &(*ptr).graph.borrow().nodes[(*ptr).index] {
        NodeKind::Oscillator { wave_type, .. } => new_js_string(ctx, wave_type),
        _ => new_js_string(ctx, ""),
    }
}

unsafe extern "C" fn wave_type_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let Some(t) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if !ptr.is_null() {
        if let NodeKind::Oscillator { wave_type, .. } =
            &mut (*ptr).graph.borrow_mut().nodes[(*ptr).index]
        {
            *wave_type = t;
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn gain_get(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue {
    let ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_float64(1.0);
    }
    match &(*ptr).graph.borrow().nodes[(*ptr).index] {
        NodeKind::Gain { gain } => sys::js_float64(*gain),
        _ => sys::js_float64(1.0),
    }
}

unsafe extern "C" fn gain_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let Some(g) = read_js_number(val) else {
        return sys::js_undefined();
    };
    if !ptr.is_null() {
        if let NodeKind::Gain { gain } = &mut (*ptr).graph.borrow_mut().nodes[(*ptr).index] {
            *gain = g;
        }
    }
    sys::js_undefined()
}

pub(super) unsafe fn ensure_node_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("AudioNode").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(node_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, NODE_CLASS_KIND, &def);

    let proto = sys::JS_NewObject(ctx);
    define_method(ctx, proto, "connect", node_connect, 1);
    define_method(ctx, proto, "start", node_start, 1);
    define_method(ctx, proto, "stop", node_stop, 1);
    define_getter_setter(ctx, proto, "frequency", frequency_get, frequency_set);
    define_getter_setter(ctx, proto, "type", wave_type_get, wave_type_set);
    define_getter_setter(ctx, proto, "gain", gain_get, gain_set);
    sys::JS_SetClassProto(ctx, class_id, proto);
    class_id
}
