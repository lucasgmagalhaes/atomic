//! `AudioBuffer` (`startRendering()`'s resolved value) — split out from
//! `web_audio/mod.rs`.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::js_helpers::define_method;

use super::helpers::read_js_number;
use super::BUFFER_CLASS_KIND;

pub(super) struct AudioBufferData {
    pub(super) channels: Vec<Vec<f64>>,
}

pub(super) unsafe fn buffer_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut AudioBufferData {
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

pub(super) unsafe fn ensure_buffer_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
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
