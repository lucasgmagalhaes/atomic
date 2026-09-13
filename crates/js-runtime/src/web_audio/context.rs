//! `OfflineAudioContext` — split out from `web_audio/mod.rs`.

use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};
use std::rc::Rc;

use quickjs_sys as sys;

use super::audio_buffer::AudioBufferData;
use super::audio_node::make_node_object;
use super::graph::{node_output, GraphState, NodeKind};
use super::helpers::{read_js_number, set_num};
use super::{BUFFER_CLASS_KIND, CONTEXT_CLASS_KIND, DESTINATION_INDEX};

pub(super) unsafe fn context_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut Rc<RefCell<GraphState>> {
    sys::JS_GetOpaque(
        this_val,
        crate::class_registry::class_id_for(rt, CONTEXT_CLASS_KIND),
    ) as *mut Rc<RefCell<GraphState>>
}

pub(super) unsafe extern "C" fn context_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = context_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

pub(super) unsafe extern "C" fn offline_audio_context_constructor(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let channels = if argc >= 1 {
        read_js_number(*argv).unwrap_or(1.0) as usize
    } else {
        1
    };
    let length = if argc >= 2 {
        read_js_number(*argv.add(1)).unwrap_or(0.0) as usize
    } else {
        0
    };
    let sample_rate = if argc >= 3 {
        read_js_number(*argv.add(2)).unwrap_or(44100.0)
    } else {
        44100.0
    };

    let graph = Rc::new(RefCell::new(GraphState {
        sample_rate,
        length,
        channels: channels.max(1),
        nodes: vec![NodeKind::Destination],
        edges: Vec::new(),
    }));

    let class_id = crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), CONTEXT_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    if new_target.tag != sys::JS_TAG_UNDEFINED {
        let proto_name = CString::new("prototype").unwrap();
        let proto = sys::JS_GetPropertyStr(ctx, new_target, proto_name.as_ptr());
        if proto.tag != sys::JS_TAG_UNDEFINED {
            sys::JS_SetPrototype(ctx, obj, proto);
        }
        sys::JS_FreeValue(ctx, proto);
    }
    sys::JS_SetOpaque(obj, Box::into_raw(Box::new(graph.clone())) as *mut c_void);

    set_num(ctx, obj, "sampleRate", sample_rate);
    set_num(ctx, obj, "length", length as f64);
    let dest = make_node_object(ctx, graph, DESTINATION_INDEX);
    let dest_name = CString::new("destination").unwrap();
    sys::JS_SetPropertyStr(ctx, obj, dest_name.as_ptr(), dest);

    obj
}

pub(super) unsafe extern "C" fn create_oscillator(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let graph = (*ptr).clone();
    let index = {
        let mut g = graph.borrow_mut();
        g.nodes.push(NodeKind::Oscillator {
            frequency: 440.0,
            wave_type: "sine".to_string(),
            start: None,
            stop: None,
        });
        g.nodes.len() - 1
    };
    make_node_object(ctx, graph, index)
}

pub(super) unsafe extern "C" fn create_gain(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let graph = (*ptr).clone();
    let index = {
        let mut g = graph.borrow_mut();
        g.nodes.push(NodeKind::Gain { gain: 1.0 });
        g.nodes.len() - 1
    };
    make_node_object(ctx, graph, index)
}

unsafe fn resolved_promise(ctx: *mut sys::JSContext, value: sys::JSValue) -> sys::JSValue {
    let mut resolving_funcs = [sys::js_undefined(); 2];
    let promise = sys::JS_NewPromiseCapability(ctx, resolving_funcs.as_mut_ptr());
    let [resolve, reject] = resolving_funcs;
    let mut arg = value;
    let result = sys::JS_Call(ctx, resolve, sys::js_undefined(), 1, &mut arg);
    sys::JS_FreeValue(ctx, result);
    sys::JS_FreeValue(ctx, value);
    sys::JS_FreeValue(ctx, resolve);
    sys::JS_FreeValue(ctx, reject);
    promise
}

pub(super) unsafe extern "C" fn start_rendering(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let ptr = context_opaque(rt, this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let graph = (*ptr).borrow();
    let mut channels = vec![Vec::with_capacity(graph.length); graph.channels];
    for i in 0..graph.length {
        let t = i as f64 / graph.sample_rate;
        let sample = node_output(&graph, DESTINATION_INDEX, t, 0);
        for ch in channels.iter_mut() {
            ch.push(sample);
        }
    }
    let sample_rate = graph.sample_rate;
    drop(graph);

    let buffer_obj = sys::JS_NewObjectClass(
        ctx,
        crate::class_registry::class_id_for(rt, BUFFER_CLASS_KIND),
    );
    if sys::js_is_exception(&buffer_obj) {
        return buffer_obj;
    }
    let numchannels = channels.len();
    let length = channels.first().map(|c| c.len()).unwrap_or(0);
    sys::JS_SetOpaque(
        buffer_obj,
        Box::into_raw(Box::new(AudioBufferData { channels })) as *mut c_void,
    );
    set_num(ctx, buffer_obj, "sampleRate", sample_rate);
    set_num(ctx, buffer_obj, "length", length as f64);
    set_num(ctx, buffer_obj, "numberOfChannels", numchannels as f64);

    resolved_promise(ctx, buffer_obj)
}
