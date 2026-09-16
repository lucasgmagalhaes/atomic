//! `element.style` — a real `CSSStyleDeclaration`-shaped object reflecting
//! the inline `style` HTML attribute (real DOM semantics: they're the same
//! backing state, in both directions — `el.setAttribute("style", "...")`
//! and `el.style.setProperty(...)` both show up through either surface).
//!
//! Declarations are parsed via plain `;`/`:` splitting rather than this
//! crate's full CSS tokenizer (`css::parse_stylesheet`) — a deliberate
//! simplification since inline style is virtually always simple
//! `prop: value` pairs; a value containing a literal `;` inside a quoted
//! string or `url(...)` (rare for inline style) would split wrong, a
//! documented scope cut rather than pulling in `css`'s full selector/value
//! grammar just to parse a flat declaration list.
//!
//! Real per-`NodeId` object identity (`node_style::STYLE_OBJECTS`, same
//! convention `dom_bindings`'s `classList`/`dataset`/`attributes` already
//! use) — freed via `cleanup`, called from `dom_bindings`'s own `cleanup`
//! on `Context` teardown. Node removal itself is non-destructive (see
//! `spec/architecture/primitives.md` §4.1), so a removed-but-still-alive
//! node's style object correctly stays cached, not evicted.
//!
//! Named camelCase accessors (`style.backgroundColor`, ...) only exist for
//! [`properties::KEBAB_PROPERTIES`] — the properties `layout-engine`'s
//! cascade resolver actually understands. Any other property name still
//! works through `setProperty`/`getPropertyValue`/`removeProperty`/
//! `cssText`/indexed iteration; it just doesn't get its own named
//! accessor, since defining one for an arbitrary property name at access
//! time would need a `Proxy` (not exposed by this crate's `quickjs-sys`
//! bindings — same scope cut `dataset`'s write side and `localStorage`'s
//! bracket access already document).
//!
//! Split into `properties.rs` (`KEBAB_PROPERTIES`/`kebab_to_camel`),
//! `declarations.rs` (inline `style` attribute parsing/serialization/get/
//! write), `accessors.rs` (`setProperty`/`getPropertyValue`/
//! `removeProperty`/`cssText`/named kebab-property accessors), and
//! `node_style.rs` (`sync_indices`/`style_owner`/`node_style_get` — the
//! per-node object identity cache) — this file keeps the shared helpers
//! and the single public entry points: `cleanup`, `define_style`.

use std::ffi::CString;

use quickjs_sys as sys;

mod accessors;
mod declarations;
mod node_style;
mod properties;

pub(crate) use node_style::define_style;

pub(super) const MAX_STYLE_LENGTH: usize = 4096;

/// Frees every cached style object for `ctx` — called from
/// `dom_bindings::cleanup`, must run before `JS_FreeContext` same as every
/// other per-context registry in this crate.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    crate::js_helpers::cleanup_object_cache(&node_style::STYLE_OBJECTS, ctx);
    crate::js_helpers::cleanup_aux_map(&node_style::STYLE_LENGTHS, ctx);
}

pub(super) unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}
pub(super) unsafe fn throw_type_error(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_string(ctx, s))
}
pub(super) unsafe fn read_string(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<String> {
    let mut len = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, value, false);
    if ptr.is_null() {
        return None;
    }
    let s = String::from_utf8_lossy(std::slice::from_raw_parts(ptr as *const u8, len)).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

pub(super) unsafe fn dom_opaque(ctx: *mut sys::JSContext) -> *mut dom::Dom {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return std::ptr::null_mut();
    }
    std::ptr::addr_of_mut!((*state).dom)
}

/// Fetches `Array.prototype` — same pattern/rationale as
/// `dom_bindings::array_prototype` (a real, spec-shaped `Symbol.iterator`/
/// `forEach`/etc for an array-like object, without this crate's
/// `quickjs-sys` bindings needing to expose well-known symbols directly).
pub(super) unsafe fn array_prototype(ctx: *mut sys::JSContext) -> sys::JSValue {
    let global = sys::JS_GetGlobalObject(ctx);
    let array_name = CString::new("Array").unwrap();
    let array_ctor = sys::JS_GetPropertyStr(ctx, global, array_name.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let proto_name = CString::new("prototype").unwrap();
    let proto = sys::JS_GetPropertyStr(ctx, array_ctor, proto_name.as_ptr());
    sys::JS_FreeValue(ctx, array_ctor);
    proto
}
