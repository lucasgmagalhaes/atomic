//! Parses the `RequestInit` subset `fetch()`/`fetch_sync` both support —
//! split out from `fetch_async/mod.rs`.

use quickjs_sys as sys;

use super::helpers::{get_prop, read_js_string};
use super::types::RequestSpec;

/// Reads the per-spec `RequestInit` subset this engine supports:
/// `method` (uppercased, default `"GET"`), `headers` (a plain object of
/// name → string value pairs), and `body` (a string, or a FormData
/// instance serialized to multipart bytes with its boundary `Content-Type`
/// auto-set unless the caller already supplied one). Unknown options and
/// non-string header values are silently ignored (documented deviation).
/// Pure JS-value parsing with no side effects, so `fetch_sync` reuses it.
pub(crate) unsafe fn read_request_init(
    ctx: *mut sys::JSContext,
    init: sys::JSValue,
) -> RequestSpec {
    let mut method = "GET".to_string();
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut body: Option<Vec<u8>> = None;
    if init.tag != sys::JS_TAG_OBJECT {
        return RequestSpec {
            method,
            headers,
            body,
        };
    }
    let method_val = get_prop(ctx, init, "method");
    if let Some(text) = read_js_string(ctx, method_val) {
        if !text.is_empty() {
            method = text.to_ascii_uppercase();
        }
    }
    sys::JS_FreeValue(ctx, method_val);

    // Caller headers first so a FormData's derived Content-Type can't
    // clobber one the script set explicitly.
    let headers_val = get_prop(ctx, init, "headers");
    if headers_val.tag == sys::JS_TAG_OBJECT {
        let mut tab: *mut sys::JSPropertyEnum = std::ptr::null_mut();
        let mut len: u32 = 0;
        if sys::JS_GetOwnPropertyNames(
            ctx,
            &mut tab,
            &mut len,
            headers_val,
            sys::JS_GPN_STRING_ENUM,
        ) >= 0
            && !tab.is_null()
        {
            for entry in std::slice::from_raw_parts(tab, len as usize) {
                let mut name_len: usize = 0;
                let name_ptr = sys::JS_AtomToCStringLen(ctx, &mut name_len, entry.atom);
                if name_ptr.is_null() {
                    continue;
                }
                let bytes = std::slice::from_raw_parts(name_ptr as *const u8, name_len);
                let key = String::from_utf8_lossy(bytes).into_owned();
                sys::JS_FreeCString(ctx, name_ptr);
                let prop_val = sys::JS_GetProperty(ctx, headers_val, entry.atom);
                if let Some(value) = read_js_string(ctx, prop_val) {
                    headers.push((key, value));
                }
                sys::JS_FreeValue(ctx, prop_val);
            }
            sys::JS_FreePropertyEnum(ctx, tab, len);
        }
    }
    sys::JS_FreeValue(ctx, headers_val);

    let body_val = get_prop(ctx, init, "body");
    match body_val.tag {
        sys::JS_TAG_STRING => {
            if let Some(text) = read_js_string(ctx, body_val) {
                // Per-spec default content type for a string body.
                if !headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                {
                    headers.push((
                        "Content-Type".to_string(),
                        "text/plain;charset=UTF-8".to_string(),
                    ));
                }
                body = Some(text.into_bytes());
            }
        }
        sys::JS_TAG_OBJECT => {
            if let Some(serialized) = crate::form_data::serialize(ctx, body_val) {
                if !headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                {
                    headers.push(("Content-Type".to_string(), serialized.content_type));
                }
                body = Some(serialized.body);
            }
        }
        _ => {}
    }
    sys::JS_FreeValue(ctx, body_val);

    RequestSpec {
        method,
        headers,
        body,
    }
}
