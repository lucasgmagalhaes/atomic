//! `OfflineAudioContext` + `OscillatorNode`/`GainNode` — the last item on
//! the spec's "Bloqueante" Web Audio line. Scoped to what
//! `OfflineAudioContext` is *for*: real, headless, deterministic audio
//! rendering into an in-memory buffer — no OS audio device involved, same
//! "headless but real" convention `render::gpu`/`render::canvas` already
//! use for pixels. A real (not the whole spec's) audio graph: real sine
//! synthesis, real gain scaling, real additive mixing at merge points,
//! real `start`/`stop` time gating — computed by `node_output`, a
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
//!
//! Split into `graph.rs` (`NodeKind`/`GraphState`/`node_output`),
//! `helpers.rs` (shared string/number/property helpers), `audio_node.rs`
//! (`AudioNode`), `audio_buffer.rs` (`AudioBuffer`), and `context.rs`
//! (`OfflineAudioContext`) — this file keeps the module doc, the class-
//! kind constants, and the single public entry point, `register`.

use std::ffi::CString;

use quickjs_sys as sys;

use crate::js_helpers::define_method;

mod audio_buffer;
mod audio_node;
mod context;
mod graph;
mod helpers;

/// See `crate::class_registry` - one registry entry per `JSRuntime` per
/// kind, not a single value shared across every `Runtime`.
const CONTEXT_CLASS_KIND: &str = "OfflineAudioContext";
const NODE_CLASS_KIND: &str = "AudioNode";
const BUFFER_CLASS_KIND: &str = "AudioBuffer";

const DESTINATION_INDEX: usize = 0;

/// Registers `OfflineAudioContext` as a global.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    audio_node::ensure_node_class(ctx);
    audio_buffer::ensure_buffer_class(ctx);

    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("OfflineAudioContext").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(context::context_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, CONTEXT_CLASS_KIND, &def);

    let proto = sys::JS_NewObject(ctx);
    define_method(
        ctx,
        proto,
        "createOscillator",
        context::create_oscillator,
        0,
    );
    define_method(ctx, proto, "createGain", context::create_gain, 0);
    define_method(ctx, proto, "startRendering", context::start_rendering, 0);
    sys::JS_SetClassProto(ctx, class_id, proto);

    let ctor_name = CString::new("OfflineAudioContext").unwrap();
    let ctor = sys::JS_NewCFunction2(
        ctx,
        context::offline_audio_context_constructor,
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
