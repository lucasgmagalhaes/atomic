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
//! Real per-`NodeId` object identity (`STYLE_OBJECTS`, same convention
//! `dom_bindings`'s `classList`/`dataset`/`attributes` already use) —
//! evicted via `evict`/`cleanup`, called from `dom_bindings`'s own
//! `evict_node_object`/`cleanup` so a removed node's cached style object
//! doesn't outlive it.
//!
//! Named camelCase accessors (`style.backgroundColor`, ...) only exist for
//! [`KEBAB_PROPERTIES`] — the properties `layout-engine`'s cascade resolver
//! actually understands. Any other property name still works through
//! `setProperty`/`getPropertyValue`/`removeProperty`/`cssText`/indexed
//! iteration; it just doesn't get its own named accessor, since defining
//! one for an arbitrary property name at access time would need a `Proxy`
//! (not exposed by this crate's `quickjs-sys` bindings — same scope cut
//! `dataset`'s write side and `localStorage`'s bracket access already
//! document).
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

const MAX_STYLE_LENGTH: usize = 4096;

/// Every inline-style property this module gives a named camelCase
/// accessor, in the exact kebab-case spelling `layout-engine::style`
/// recognizes — keep in sync with that resolver's own property list.
const KEBAB_PROPERTIES: &[&str] = &[
    "display",
    "position",
    "top",
    "right",
    "bottom",
    "left",
    "width",
    "height",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "background-color",
    "background",
    "color",
    "font-size",
    "border-width",
    "border-style",
    "border-color",
    "overflow",
    "float",
    "clear",
    "flex-direction",
    "justify-content",
    "align-items",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
];

thread_local! {
    static STYLE_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
    /// Last synced declaration count per `(ctx, NodeId)` style object, so
    /// `sync_indices` knows how many trailing indices (from a shrunk
    /// declaration list) need clearing to `undefined` — same convention
    /// `dom_bindings::CLASS_LIST_LENGTHS`/`ATTRS_LENGTHS` already use.
    static STYLE_LENGTHS: RefCell<HashMap<usize, HashMap<dom::NodeId, usize>>> = RefCell::new(HashMap::new());
}

/// Evicts `id`'s cached style object for `ctx`, if any — called from
/// `dom_bindings::evict_node_object` alongside its own caches, so a removed
/// node's style object doesn't outlive it.
pub(crate) unsafe fn evict(ctx: *mut sys::JSContext, id: dom::NodeId) {
    let cached = STYLE_OBJECTS.with(|reg| {
        reg.borrow_mut()
            .get_mut(&(ctx as usize))
            .and_then(|nodes| nodes.remove(&id))
    });
    if let Some(object) = cached {
        sys::JS_FreeValue(ctx, object);
    }
    STYLE_LENGTHS.with(|reg| {
        if let Some(nodes) = reg.borrow_mut().get_mut(&(ctx as usize)) {
            nodes.remove(&id);
        }
    });
}

/// Frees every cached style object for `ctx` — called from
/// `dom_bindings::cleanup`, must run before `JS_FreeContext` same as every
/// other per-context registry in this crate.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    if let Some(objects) = STYLE_OBJECTS.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
        for (_, object) in objects {
            sys::JS_FreeValue(ctx, object);
        }
    }
    STYLE_LENGTHS.with(|reg| {
        reg.borrow_mut().remove(&(ctx as usize));
    });
}

unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}
unsafe fn throw_type_error(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_string(ctx, s))
}
unsafe fn read_string(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<String> {
    let mut len = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, value, false);
    if ptr.is_null() {
        return None;
    }
    let s = String::from_utf8_lossy(std::slice::from_raw_parts(ptr as *const u8, len)).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

unsafe fn dom_opaque(ctx: *mut sys::JSContext) -> *mut dom::Dom {
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
unsafe fn array_prototype(ctx: *mut sys::JSContext) -> sys::JSValue {
    let global = sys::JS_GetGlobalObject(ctx);
    let array_name = CString::new("Array").unwrap();
    let array_ctor = sys::JS_GetPropertyStr(ctx, global, array_name.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let proto_name = CString::new("prototype").unwrap();
    let proto = sys::JS_GetPropertyStr(ctx, array_ctor, proto_name.as_ptr());
    sys::JS_FreeValue(ctx, array_ctor);
    proto
}

/// `background-color` -> `backgroundColor`, the same mapping
/// `dom_bindings::kebab_to_camel` already does for `dataset` — duplicated
/// locally rather than shared, matching this crate's convention of small
/// per-module helpers over a shared-utility module (see e.g.
/// `location.rs`/`history.rs`'s own local `read_string`/`new_string`).
fn kebab_to_camel(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut capitalize = false;
    for ch in name.chars() {
        if ch == '-' {
            capitalize = true;
            continue;
        }
        if capitalize {
            result.extend(ch.to_uppercase());
            capitalize = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Parses `text` (an inline `style` attribute value) into an ordered list
/// of `(property, value)` pairs — see the module doc for the plain
/// `;`/`:`-splitting scope cut. Malformed segments (no `:`, an empty name/
/// value) are silently skipped rather than erroring, matching how a real
/// browser tolerates garbage in a `style` attribute rather than throwing.
fn parse_declarations(text: &str) -> Vec<(String, String)> {
    text.split(';')
        .filter_map(|part| {
            let mut split = part.splitn(2, ':');
            let name = split.next()?.trim();
            let value = split.next()?.trim();
            if name.is_empty() || value.is_empty() {
                None
            } else {
                Some((name.to_string(), value.to_string()))
            }
        })
        .collect()
}

fn serialize_declarations(decls: &[(String, String)]) -> String {
    decls
        .iter()
        .map(|(name, value)| format!("{name}: {value};"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The last declared value for `name` in `id`'s `style` attribute, or
/// `None` if absent — `rev()` so a later duplicate declaration for the same
/// property (real, if rare, CSS: `color: red; color: blue;`) wins, matching
/// cascade-within-one-declaration-block semantics.
unsafe fn get_declaration(dom: *const dom::Dom, id: dom::NodeId, name: &str) -> Option<String> {
    let text = (*dom).attribute(id, "style").unwrap_or_default();
    parse_declarations(text)
        .into_iter()
        .rev()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v)
}

/// Sets (`Some(value)`) or removes (`None`) `name` in `id`'s `style`
/// attribute, rewriting the whole attribute — every existing declaration
/// for `name` is dropped first (not just the last one), so a garbled
/// `color:red;color:blue` collapses to a single clean declaration on the
/// next write.
unsafe fn write_declaration(dom: *mut dom::Dom, id: dom::NodeId, name: &str, value: Option<&str>) {
    let text = (*dom)
        .attribute(id, "style")
        .unwrap_or_default()
        .to_string();
    let mut decls = parse_declarations(&text);
    decls.retain(|(n, _)| n != name);
    if let Some(value) = value {
        decls.push((name.to_string(), value.to_string()));
    }
    (*dom).set_attribute(id, "style", &serialize_declarations(&decls));
}

unsafe fn style_owner(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<dom::NodeId> {
    let name = CString::new("__nimbleStyleOwner").unwrap();
    let owner = sys::JS_GetPropertyStr(ctx, value, name.as_ptr());
    let id = crate::dom_bindings::node_id(ctx, owner);
    sys::JS_FreeValue(ctx, owner);
    id
}

/// Refreshes a style object's indexed properties (`0`, `1`, ...) — each
/// holding a declared property's own name, real `CSSStyleDeclaration`
/// indexed-access semantics — and `length` from the node's live `style`
/// attribute, clearing any trailing index left over from a longer previous
/// declaration list. Called on every mutation and every `element.style`
/// getter hit, same convention `dom_bindings::sync_class_list` uses.
unsafe fn sync_indices(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
    object: sys::JSValue,
) {
    let text = (*dom)
        .attribute(id, "style")
        .unwrap_or_default()
        .to_string();
    let decls = parse_declarations(&text);
    let old_len = STYLE_LENGTHS
        .with(|reg| {
            reg.borrow()
                .get(&(ctx as usize))
                .and_then(|nodes| nodes.get(&id).copied())
        })
        .unwrap_or(0);
    for (index, (name, _)) in decls.iter().enumerate() {
        sys::JS_SetPropertyUint32(ctx, object, index as u32, new_string(ctx, name));
    }
    for index in decls.len()..old_len {
        sys::JS_SetPropertyUint32(ctx, object, index as u32, sys::js_undefined());
    }
    let length_name = CString::new("length").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        object,
        length_name.as_ptr(),
        sys::js_float64(decls.len() as f64),
    );
    STYLE_LENGTHS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, decls.len())
    });
}

unsafe extern "C" fn set_property(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "setProperty requires a name and a value");
    }
    let Some(name) = read_string(ctx, *argv) else {
        return throw_type_error(ctx, "property name must be a string");
    };
    let Some(value) = read_string(ctx, *argv.add(1)) else {
        return throw_type_error(ctx, "property value must be a string");
    };
    if name.len() > 128 || value.len() > MAX_STYLE_LENGTH {
        return throw_type_error(ctx, "style property or value exceeds the maximum length");
    }
    let Some(id) = style_owner(ctx, this_val) else {
        return throw_type_error(ctx, "invalid style receiver");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    write_declaration(dom, id, &name, Some(&value));
    sync_indices(ctx, dom, id, this_val);
    sys::js_undefined()
}

unsafe extern "C" fn get_property_value(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return new_string(ctx, "");
    }
    let Some(name) = read_string(ctx, *argv) else {
        return new_string(ctx, "");
    };
    let Some(id) = style_owner(ctx, this_val) else {
        return new_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return new_string(ctx, "");
    }
    new_string(ctx, &get_declaration(dom, id, &name).unwrap_or_default())
}

unsafe extern "C" fn remove_property(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return new_string(ctx, "");
    }
    let Some(name) = read_string(ctx, *argv) else {
        return new_string(ctx, "");
    };
    let Some(id) = style_owner(ctx, this_val) else {
        return new_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return new_string(ctx, "");
    }
    let old = get_declaration(dom, id, &name).unwrap_or_default();
    write_declaration(dom, id, &name, None);
    sync_indices(ctx, dom, id, this_val);
    new_string(ctx, &old)
}

type Getter = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue) -> sys::JSValue;
type Setter = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue, sys::JSValue) -> sys::JSValue;
type GetterMagic = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue, c_int) -> sys::JSValue;
type SetterMagic =
    unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue, sys::JSValue, c_int) -> sys::JSValue;

unsafe extern "C" fn css_text_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = style_owner(ctx, this_val) else {
        return new_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return new_string(ctx, "");
    }
    new_string(ctx, (*dom).attribute(id, "style").unwrap_or_default())
}

unsafe extern "C" fn css_text_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(text) = read_string(ctx, val) else {
        return throw_type_error(ctx, "cssText must be a string");
    };
    if text.len() > MAX_STYLE_LENGTH {
        return throw_type_error(ctx, "cssText exceeds the maximum length");
    }
    let Some(id) = style_owner(ctx, this_val) else {
        return throw_type_error(ctx, "invalid style receiver");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).set_attribute(id, "style", &text);
    sync_indices(ctx, dom, id, this_val);
    sys::js_undefined()
}

unsafe extern "C" fn named_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    magic: c_int,
) -> sys::JSValue {
    let Some(&name) = KEBAB_PROPERTIES.get(magic as usize) else {
        return sys::js_undefined();
    };
    let Some(id) = style_owner(ctx, this_val) else {
        return new_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return new_string(ctx, "");
    }
    new_string(ctx, &get_declaration(dom, id, name).unwrap_or_default())
}

unsafe extern "C" fn named_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
    magic: c_int,
) -> sys::JSValue {
    let Some(&name) = KEBAB_PROPERTIES.get(magic as usize) else {
        return sys::js_undefined();
    };
    let Some(value) = read_string(ctx, val) else {
        return throw_type_error(ctx, "style property value must be a string");
    };
    if value.len() > MAX_STYLE_LENGTH {
        return throw_type_error(ctx, "style value exceeds the maximum length");
    }
    let Some(id) = style_owner(ctx, this_val) else {
        return throw_type_error(ctx, "invalid style receiver");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    write_declaration(dom, id, name, Some(&value));
    sync_indices(ctx, dom, id, this_val);
    sys::js_undefined()
}

unsafe extern "C" fn node_style_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = crate::dom_bindings::node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    if let Some(value) = STYLE_OBJECTS.with(|reg| {
        reg.borrow()
            .get(&(ctx as usize))
            .and_then(|nodes| nodes.get(&id).copied())
    }) {
        let dom = dom_opaque(ctx);
        if !dom.is_null() {
            sync_indices(ctx, dom, id, value);
        }
        return sys::JS_DupValue(ctx, value);
    }

    let object = sys::JS_NewObject(ctx);
    let proto = array_prototype(ctx);
    sys::JS_SetPrototype(ctx, object, proto);
    sys::JS_FreeValue(ctx, proto);

    let owner_name = CString::new("__nimbleStyleOwner").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        object,
        owner_name.as_ptr(),
        sys::JS_DupValue(ctx, this_val),
    );

    for (name, function, arity) in [
        ("setProperty", set_property as sys::JSCFunction, 2),
        (
            "getPropertyValue",
            get_property_value as sys::JSCFunction,
            1,
        ),
        ("removeProperty", remove_property as sys::JSCFunction, 1),
    ] {
        let cname = CString::new(name).unwrap();
        sys::JS_SetPropertyStr(
            ctx,
            object,
            cname.as_ptr(),
            sys::JS_NewCFunction2(
                ctx,
                function,
                cname.as_ptr(),
                arity,
                sys::JS_CFUNC_GENERIC,
                0,
            ),
        );
    }

    let css_text_name = CString::new("cssText").unwrap();
    let css_text_getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(css_text_get),
        css_text_name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let css_text_setter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(css_text_set),
        css_text_name.as_ptr(),
        1,
        sys::JS_CFUNC_SETTER,
        0,
    );
    let css_text_atom = sys::JS_NewAtom(ctx, css_text_name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        object,
        css_text_atom,
        css_text_getter,
        css_text_setter,
        sys::JS_PROP_HAS_GET | sys::JS_PROP_HAS_SET | sys::JS_PROP_CONFIGURABLE,
    );
    sys::JS_FreeAtom(ctx, css_text_atom);

    for (index, kebab) in KEBAB_PROPERTIES.iter().enumerate() {
        let camel = kebab_to_camel(kebab);
        let cname = CString::new(camel).unwrap();
        let getter = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<GetterMagic, sys::JSCFunction>(named_get),
            cname.as_ptr(),
            0,
            sys::JS_CFUNC_GETTER_MAGIC,
            index as c_int,
        );
        let setter = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<SetterMagic, sys::JSCFunction>(named_set),
            cname.as_ptr(),
            1,
            sys::JS_CFUNC_SETTER_MAGIC,
            index as c_int,
        );
        let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
        sys::JS_DefinePropertyGetSet(
            ctx,
            object,
            atom,
            getter,
            setter,
            sys::JS_PROP_HAS_GET | sys::JS_PROP_HAS_SET | sys::JS_PROP_CONFIGURABLE,
        );
        sys::JS_FreeAtom(ctx, atom);
    }

    let dom = dom_opaque(ctx);
    if !dom.is_null() {
        sync_indices(ctx, dom, id, object);
    }
    STYLE_OBJECTS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, sys::JS_DupValue(ctx, object))
    });
    object
}

/// Defines the `style` accessor on `proto` (`Node.prototype`) — read-only
/// at the property level (there's no `element.style = "..."` real setter;
/// only `cssText`/individual named properties/`setProperty` mutate it),
/// matching real DOM: `Element.prototype.style` itself has no setter
/// either.
pub(crate) unsafe fn define_style(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("style").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(node_style_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}
