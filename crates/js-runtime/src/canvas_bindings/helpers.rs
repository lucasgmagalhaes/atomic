//! Small JS-value read/write helpers shared across `canvas_bindings`'s
//! submodules - string/number extraction, plain-object property access,
//! and the `#rgb`/`#rrggbb` hex color parser/formatter every color
//! getter/setter in this module uses.
use std::ffi::CString;

use quickjs_sys as sys;

use layout_engine::Color;

pub(super) unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
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

pub(super) unsafe fn read_js_f32(val: sys::JSValue) -> f32 {
    match val.tag {
        sys::JS_TAG_INT => val.u.int32 as f32,
        sys::JS_TAG_FLOAT64 => val.u.float64 as f32,
        _ => 0.0,
    }
}

pub(super) unsafe fn get_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    key: &str,
) -> sys::JSValue {
    let name = CString::new(key).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

pub(super) unsafe fn set_prop_f64(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    key: &str,
    val: f64,
) {
    let name = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), sys::js_float64(val));
}

/// `#rgb`/`#rrggbb` only - no `rgb()`/`rgba()`/`hsl()`/named color
/// keywords. Neither `render::Canvas2D` nor a publicly exposed
/// `layout_engine` API has a general CSS-color-string parser to reuse
/// (the one `layout-engine` has, `Color::named`/`from_hex`, is
/// `pub(super)`-private to its own style-cascade module) - this is a
/// small, self-contained parser scoped to the one real-world-common
/// pattern (`ctx.fillStyle = "#ff0000"`), matching this crate's own
/// "narrower than spec, clearly documented" convention.
pub(super) fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    let (r, g, b) = match hex.len() {
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ),
        3 => {
            let double = |c: char| u8::from_str_radix(&format!("{c}{c}"), 16).ok();
            let mut chars = hex.chars();
            (
                double(chars.next()?)?,
                double(chars.next()?)?,
                double(chars.next()?)?,
            )
        }
        _ => return None,
    };
    Some(Color { r, g, b, a: 255 })
}

pub(super) fn format_hex_color(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}
