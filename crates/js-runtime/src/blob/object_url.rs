//! `URL.createObjectURL`/`revokeObjectURL` and the per-context blob-URL
//! table — split out from `blob.rs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::os::raw::c_int;
use std::sync::atomic::{AtomicU64, Ordering};

use quickjs_sys as sys;

use super::helpers::{new_js_string, read_js_string};
use super::{blob_opaque, BlobInner};

static NEXT_BLOB_URL_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    // Keyed by JSContext pointer, same reasoning as `fetch_async::STATE`.
    static OBJECT_URLS: RefCell<HashMap<usize, HashMap<String, BlobInner>>> = RefCell::new(HashMap::new());
}

pub(super) unsafe extern "C" fn create_object_url(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let ptr = blob_opaque(sys::JS_GetRuntime(ctx), *argv);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let id = NEXT_BLOB_URL_ID.fetch_add(1, Ordering::Relaxed);
    let url = format!("blob:atomic-internal/{id:016x}");
    let entry = BlobInner {
        bytes: (*ptr).bytes.clone(),
        mime: (*ptr).mime.clone(),
    };
    OBJECT_URLS.with(|m| {
        m.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(url.clone(), entry);
    });
    new_js_string(ctx, &url)
}

pub(super) unsafe extern "C" fn revoke_object_url(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc >= 1 {
        if let Some(url) = read_js_string(ctx, *argv) {
            OBJECT_URLS.with(|m| {
                if let Some(table) = m.borrow_mut().get_mut(&(ctx as usize)) {
                    table.remove(&url);
                }
            });
        }
    }
    sys::js_undefined()
}

/// Drops this context's `URL.createObjectURL` table (avoids leaking one
/// entry per `JSContext` pointer across the process, same reasoning as
/// `timers::cleanup`/`fetch_async::cleanup`).
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    OBJECT_URLS.with(|m| {
        m.borrow_mut().remove(&(ctx as usize));
    });
}
