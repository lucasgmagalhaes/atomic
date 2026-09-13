//! `Headers`' shared init-value parser — split out of `headers.rs` to
//! keep it under this project's per-file line convention. Still
//! genuinely part of the `Headers` class's own logic (a `Headers`
//! constructor arg parses through it too), and reused as-is by
//! `Request`/`Response`'s own constructors (`request.rs`/`response.rs`
//! accept the same `headers` init shape), hence `pub(super)`.

use quickjs_sys as sys;

use super::headers::headers_opaque;
use super::{get_prop, read_js_string};

/// Parse a JS init value into header entries. Accepts:
/// - A plain object: `{ "Content-Type": "text/html" }`
/// - An array of arrays: `[["Content-Type", "text/html"]]`
/// - A Headers instance (copies its entries)
pub(super) unsafe fn parse_headers_init(
    ctx: *mut sys::JSContext,
    init: sys::JSValue,
) -> Vec<(String, String)> {
    let mut entries = Vec::new();

    if init.tag == sys::JS_TAG_UNDEFINED || init.tag == sys::JS_TAG_NULL {
        return entries;
    }

    // Headers instance — copy its entries via opaque
    let opaque = headers_opaque(sys::JS_GetRuntime(ctx), init);
    if !opaque.is_null() {
        return (*opaque).entries.clone();
    }

    if init.tag != sys::JS_TAG_OBJECT {
        return entries;
    }

    // Check if it's an array (check constructor name)
    let ctor_name = get_prop(ctx, init, "constructor");
    if ctor_name.tag == sys::JS_TAG_OBJECT {
        let name_str = get_prop(ctx, ctor_name, "name");
        if let Some(name) = read_js_string(ctx, name_str) {
            if name == "Array" {
                // Array of [name, value] pairs
                let len_val = get_prop(ctx, init, "length");
                let len = if len_val.tag == sys::JS_TAG_INT {
                    len_val.u.int32 as u32
                } else {
                    0
                };
                sys::JS_FreeValue(ctx, len_val);
                for i in 0..len {
                    let pair = sys::JS_GetPropertyUint32(ctx, init, i);
                    if pair.tag == sys::JS_TAG_OBJECT {
                        let name_val = sys::JS_GetPropertyUint32(ctx, pair, 0);
                        let val_val = sys::JS_GetPropertyUint32(ctx, pair, 1);
                        if let (Some(name), Some(value)) =
                            (read_js_string(ctx, name_val), read_js_string(ctx, val_val))
                        {
                            entries.push((name, value));
                        }
                        sys::JS_FreeValue(ctx, name_val);
                        sys::JS_FreeValue(ctx, val_val);
                    }
                    sys::JS_FreeValue(ctx, pair);
                }
                sys::JS_FreeValue(ctx, name_str);
                sys::JS_FreeValue(ctx, ctor_name);
                return entries;
            }
        }
        sys::JS_FreeValue(ctx, name_str);
    }
    sys::JS_FreeValue(ctx, ctor_name);

    // Plain object — enumerate string keys
    let mut tab: *mut sys::JSPropertyEnum = std::ptr::null_mut();
    let mut len: u32 = 0;
    if sys::JS_GetOwnPropertyNames(ctx, &mut tab, &mut len, init, sys::JS_GPN_STRING_ENUM) >= 0
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
            let prop_val = sys::JS_GetProperty(ctx, init, entry.atom);
            if let Some(value) = read_js_string(ctx, prop_val) {
                entries.push((key, value));
            }
            sys::JS_FreeValue(ctx, prop_val);
        }
        sys::JS_FreePropertyEnum(ctx, tab, len);
    }

    entries
}
