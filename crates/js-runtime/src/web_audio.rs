//! `OfflineAudioContext` + `OscillatorNode`/`GainNode` — the last item on
//! the spec's "Bloqueante" Web Audio line. Scoped to what
//! `OfflineAudioContext` is *for*: real, headless, deterministic audio
//! rendering into an in-memory buffer — no OS audio device involved, same
//! "headless but real" convention `render::gpu`/`render::canvas` already
//! use for pixels. A real (not the whole spec's) audio graph: real sine
//! synthesis, real gain scaling, real additive mixing at merge points,
//! real `start`/`stop` time gating — computed by [`node_output`], a
//! small recursive signal-flow evaluator over the graph `connect()`
//! builds.
//!
//! Deviations, all documented at the point they matter: only
//! `OfflineAudioContext` exists (no live `AudioContext`/real-time
//! playback — there's no audio output device concept anywhere in this
//! engine, matching `render`'s own "headless texture readback" scope);
//! only sine-wave oscillators actually synthesize (`type` is stored and
//! read back honestly, but every type renders as sine); no filters/
//! panners/analysers/`AudioParam` automation curves (`frequency`/`gain`
//! are plain scalars, not ramp-able `AudioParam`s); `AudioBuffer.
//! getChannelData` returns a plain JS `Array` of numbers, not a real
//! `Float32Array` (no `JS_NewFloat32Array` binding exists yet, only
//! `Uint8Array`'s); no cycle detection beyond a hard recursion-depth cap
//! (a feedback loop renders as silence past the cap instead of hanging).
use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};
use std::rc::Rc;

use quickjs_sys as sys;

use crate::js_helpers::{define_getter_setter, define_method};

/// See `crate::class_registry` - one registry entry per `JSRuntime` per
/// kind, not a single value shared across every `Runtime`.
const CONTEXT_CLASS_KIND: &str = "OfflineAudioContext";
const NODE_CLASS_KIND: &str = "AudioNode";
const BUFFER_CLASS_KIND: &str = "AudioBuffer";

const DESTINATION_INDEX: usize = 0;
const MAX_GRAPH_DEPTH: u32 = 32;

enum NodeKind {
    Destination,
    Oscillator {
        frequency: f64,
        wave_type: String,
        start: Option<f64>,
        stop: Option<f64>,
    },
    Gain {
        gain: f64,
    },
}

struct GraphState {
    sample_rate: f64,
    length: usize,
    channels: usize,
    nodes: Vec<NodeKind>,
    edges: Vec<(usize, usize)>,
}

/// Real recursive signal-flow evaluation: a node's output at time `t` is
/// a function of whatever feeds into it (sum of every edge's source,
/// scaled by gain for a `Gain` node, silence outside `[start, stop)` for
/// an `Oscillator`) — the same additive-mixing-at-merge-points model a
/// real Web Audio graph uses, just computed on demand per sample instead
/// of once per render quantum.
fn node_output(graph: &GraphState, idx: usize, t: f64, depth: u32) -> f64 {
    if depth > MAX_GRAPH_DEPTH {
        return 0.0;
    }
    match &graph.nodes[idx] {
        NodeKind::Oscillator {
            frequency,
            start,
            stop,
            ..
        } => {
            let started = start.map(|s| t >= s).unwrap_or(true);
            let stopped = stop.map(|s| t >= s).unwrap_or(false);
            if started && !stopped {
                (2.0 * std::f64::consts::PI * frequency * t).sin()
            } else {
                0.0
            }
        }
        NodeKind::Gain { gain } => {
            let sum: f64 = graph
                .edges
                .iter()
                .filter(|(_, to)| *to == idx)
                .map(|(from, _)| node_output(graph, *from, t, depth + 1))
                .sum();
            sum * gain
        }
        NodeKind::Destination => graph
            .edges
            .iter()
            .filter(|(_, to)| *to == idx)
            .map(|(from, _)| node_output(graph, *from, t, depth + 1))
            .sum(),
    }
}

unsafe fn read_js_number(val: sys::JSValue) -> Option<f64> {
    match val.tag {
        sys::JS_TAG_INT => Some(val.u.int32 as f64),
        sys::JS_TAG_FLOAT64 => Some(val.u.float64),
        _ => None,
    }
}

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
    let mut len: usize = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, val, false);
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
    let s = String::from_utf8_lossy(bytes).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe fn set_num(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: f64) {
    let name = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), sys::js_float64(val));
}

// ---- AudioNode (Oscillator/Gain/Destination share one opaque shape) ----

struct AudioNodeData {
    graph: Rc<RefCell<GraphState>>,
    index: usize,
}

unsafe fn node_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut AudioNodeData {
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

unsafe fn make_node_object(
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

unsafe fn ensure_node_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
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

// ---- AudioBuffer (startRendering()'s resolved value) ----

struct AudioBufferData {
    channels: Vec<Vec<f64>>,
}

unsafe fn buffer_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut AudioBufferData {
    sys::JS_GetOpaque(
        this_val,
        crate::class_registry::class_id_for(rt, BUFFER_CLASS_KIND),
    ) as *mut AudioBufferData
}

unsafe extern "C" fn buffer_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = buffer_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

unsafe extern "C" fn get_channel_data(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = buffer_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::JS_NewArray(ctx);
    }
    let channel = if argc >= 1 {
        read_js_number(*argv).unwrap_or(0.0) as usize
    } else {
        0
    };
    let Some(samples) = (&(*ptr).channels).get(channel) else {
        return sys::JS_NewArray(ctx);
    };

    let arr = sys::JS_NewArray(ctx);
    for (i, sample) in samples.iter().enumerate() {
        sys::JS_SetPropertyUint32(ctx, arr, i as u32, sys::js_float64(*sample));
    }
    arr
}

unsafe fn ensure_buffer_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("AudioBuffer").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(buffer_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, BUFFER_CLASS_KIND, &def);

    let proto = sys::JS_NewObject(ctx);
    define_method(ctx, proto, "getChannelData", get_channel_data, 1);
    sys::JS_SetClassProto(ctx, class_id, proto);
    class_id
}

// ---- OfflineAudioContext ----

unsafe fn context_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut Rc<RefCell<GraphState>> {
    sys::JS_GetOpaque(
        this_val,
        crate::class_registry::class_id_for(rt, CONTEXT_CLASS_KIND),
    ) as *mut Rc<RefCell<GraphState>>
}

unsafe extern "C" fn context_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = context_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

unsafe extern "C" fn offline_audio_context_constructor(
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

unsafe extern "C" fn create_oscillator(
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

unsafe extern "C" fn create_gain(
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

unsafe extern "C" fn start_rendering(
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

/// Registers `OfflineAudioContext` as a global.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    ensure_node_class(ctx);
    ensure_buffer_class(ctx);

    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("OfflineAudioContext").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(context_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, CONTEXT_CLASS_KIND, &def);

    let proto = sys::JS_NewObject(ctx);
    define_method(ctx, proto, "createOscillator", create_oscillator, 0);
    define_method(ctx, proto, "createGain", create_gain, 0);
    define_method(ctx, proto, "startRendering", start_rendering, 0);
    sys::JS_SetClassProto(ctx, class_id, proto);

    let ctor_name = CString::new("OfflineAudioContext").unwrap();
    let ctor = sys::JS_NewCFunction2(
        ctx,
        offline_audio_context_constructor,
        ctor_name.as_ptr(),
        3,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(ctx, ctor, proto_name.as_ptr(), sys::JS_DupValue(ctx, proto));

    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, ctor_name.as_ptr(), ctor);
    sys::JS_FreeValue(ctx, global);
}
