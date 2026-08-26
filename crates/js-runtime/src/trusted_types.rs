//! Trusted Types, scoped to what this engine actually has: a real
//! `trustedTypes.createPolicy(name, handlers)` factory producing policy
//! objects whose `createHTML(string)` wraps its result in an opaque
//! `TrustedHTML` instance, plus enforcement of the CSP directive
//! `require-trusted-types-for 'script'` over the DOM injection sinks this
//! engine actually exposes — `innerHTML`/`outerHTML` setters
//! (`dom_bindings`). Under enforcement those setters reject every plain
//! string with a TypeError (the same failure shape a real browser
//! produces) and accept only a policy-produced `TrustedHTML`; without the
//! directive they behave exactly as before, and a `TrustedHTML` value is
//! accepted either way.
//!
//! Delivery rides the existing CSP plumbing for free: policies arrive via
//! `Context::set_csp`/`add_csp_policy` (profile-worker delivers response
//! headers and `<meta http-equiv>` tags before any page script runs), and
//! [`crate::csp::is_trusted_types_required`] re-reads them per sink hit,
//! so multiple delivered policies behave like everywhere else in
//! `csp.rs` — if *any* delivered policy requires trusted types,
//! enforcement is on.
//!
//! Scope cuts (each mirrors a gap elsewhere in this crate rather than a
//! half-built abstraction): only `createHTML` exists on a policy — there
//! are no script/URL injection sinks in this engine to gate (`eval` isn't
//! a Trusted Types sink even in real browsers), so no `TrustedScript`/
//! `TrustedScriptURL` types are introduced. The `trusted-types <names>`
//! allowlist directive is not enforced (any policy name is creatable);
//! `defaultPolicy` always reads null (no default-policy constructor
//! concept exists here); `getAttributeType`/`getPropertyType` mappings
//! aren't exposed (the only sink type is HTML); policy callbacks must be
//! invoked as methods (`policy.createHTML(x)` — a destructured reference
//! loses its handler, unlike a real bound policy object); and on a bare
//! `Context::new` (no host state) duplicate policy names aren't tracked.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

const TRUSTED_HTML_KIND: &str = "TrustedHTML";

unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
    let mut len: usize = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, val, false);
    if ptr.is_null() {
        return None;
    }
    let s = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

unsafe fn throw_type_error(ctx: *mut sys::JSContext, message: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_js_string(ctx, message))
}

/// Reads the data behind a real `TrustedHTML` instance, or `None` for any
/// other value — the one check both the sink gate ([`sink_html_string`])
/// and anything else that needs a typed value's contents go through.
pub(crate) unsafe fn trusted_html_data(
    ctx: *mut sys::JSContext,
    val: sys::JSValue,
) -> Option<String> {
    let class_id = crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), TRUSTED_HTML_KIND);
    let ptr = sys::JS_GetOpaque(val, class_id) as *mut String;
    (!ptr.is_null()).then(|| (*ptr).clone())
}

/// The one seam every HTML injection sink routes through: accepts a real
/// `TrustedHTML` unconditionally (typed values are the whole point of the
/// API, enforced or not), otherwise coerces the value to a string and —
/// only if a delivered policy requires trusted types — throws the same
/// TypeError a real browser raises for a string assigned to an enforced
/// sink. `Err` carries an already-thrown exception value the setter
/// returns directly.
pub(crate) unsafe fn sink_html_string(
    ctx: *mut sys::JSContext,
    val: sys::JSValue,
    sink_name: &str,
) -> Result<String, sys::JSValue> {
    if let Some(data) = trusted_html_data(ctx, val) {
        return Ok(data);
    }
    match read_js_string(ctx, val) {
        Some(html) => {
            if crate::csp::is_trusted_types_required(ctx) {
                Err(throw_type_error(
                    ctx,
                    &format!("Failed to set the '{sink_name}' property on 'Element': This document requires 'TrustedHTML' assignment."),
                ))
            } else {
                Ok(html)
            }
        }
        None => Err(throw_type_error(
            ctx,
            &format!("{sink_name} must be a string or TrustedHTML"),
        )),
    }
}

unsafe extern "C" fn trusted_html_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let class_id = crate::class_registry::class_id_for(rt, TRUSTED_HTML_KIND);
    let ptr = sys::JS_GetOpaque(val, class_id) as *mut String;
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

/// `String(trustedHtml)` / template / concatenation paths — the typed
/// wrapper's data, so a policy-wrapped value reads back exactly what the
/// policy approved.
unsafe extern "C" fn trusted_html_to_string(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    match trusted_html_data(ctx, this_val) {
        Some(data) => new_js_string(ctx, &data),
        None => throw_type_error(ctx, "invalid TrustedHTML receiver"),
    }
}

fn make_trusted_html(ctx: *mut sys::JSContext, data: String) -> sys::JSValue {
    unsafe {
        let rt = sys::JS_GetRuntime(ctx);
        let class_id = crate::class_registry::class_id_for(rt, TRUSTED_HTML_KIND);
        let obj = sys::JS_NewObjectClass(ctx, class_id);
        if sys::js_is_exception(&obj) {
            return obj;
        }
        sys::JS_SetOpaque(
            obj,
            Box::into_raw(Box::new(data)) as *mut std::os::raw::c_void,
        );
        obj
    }
}

/// `policy.createHTML(input)` — calls the user-provided handler stored on
/// the policy object (see [`register`]'s docs for why the handler lives as
/// a property of the policy itself), coerces its result to a string, and
/// wraps it in a fresh opaque `TrustedHTML`. A policy created without a
/// `createHTML` handler throws, matching the real API's behavior for a
/// missing handler with no default policy installed.
unsafe extern "C" fn policy_create_html(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let handler_name = CString::new("_createHTML").unwrap();
    let handler = sys::JS_GetPropertyStr(ctx, this_val, handler_name.as_ptr());
    if !sys::JS_IsFunction(ctx, handler) {
        sys::JS_FreeValue(ctx, handler);
        return throw_type_error(ctx, "This policy does not include a createHTML handler");
    }
    let input = if argc >= 1 {
        sys::JS_DupValue(ctx, *argv)
    } else {
        sys::js_undefined()
    };
    let mut args = [input];
    let result = sys::JS_Call(ctx, handler, sys::js_undefined(), 1, args.as_mut_ptr());
    sys::JS_FreeValue(ctx, args[0]);
    sys::JS_FreeValue(ctx, handler);
    if sys::js_is_exception(&result) {
        return result;
    }
    let Some(text) = read_js_string(ctx, result) else {
        sys::JS_FreeValue(ctx, result);
        return throw_type_error(
            ctx,
            "createHTML handler did not return a stringifiable value",
        );
    };
    sys::JS_FreeValue(ctx, result);
    make_trusted_html(ctx, text)
}

/// `trustedTypes.createPolicy(name, handlers)` — rejects empty names and
/// duplicates of an already-created name (per-context, matching a real
/// document's policy-name uniqueness rule), requires a callable
/// `createHTML` when a handlers object supplies one, and returns a plain
/// object carrying `name` plus the live `createHTML` method. Only
/// `createHTML` is recognized in `handlers`; other keys are ignored (no
/// sinks exist for their types — see the module docs).
unsafe extern "C" fn create_policy(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "createPolicy requires a policy name");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "createPolicy name must be a string");
    };
    if name.is_empty() {
        return throw_type_error(ctx, "createPolicy name must not be empty");
    }

    let mut handler = sys::js_undefined();
    if argc >= 2 {
        let rules = argv.add(1).read();
        if rules.tag == sys::JS_TAG_OBJECT {
            let key = CString::new("createHTML").unwrap();
            handler = sys::JS_GetPropertyStr(ctx, rules, key.as_ptr());
            if handler.tag != sys::JS_TAG_UNDEFINED && !sys::JS_IsFunction(ctx, handler) {
                sys::JS_FreeValue(ctx, handler);
                return throw_type_error(ctx, "createPolicy createHTML handler must be a function");
            }
        }
    }
    // A missing/absent handlers object leaves `handler` undefined; the
    // policy still gets created (its createHTML will throw when called),
    // matching how a real policy without that handler behaves once called.

    let state = crate::host_state::get(ctx);
    if !state.is_null() {
        if (*state).trusted_type_policy_names.contains(&name) {
            sys::JS_FreeValue(ctx, handler);
            return throw_type_error(ctx, &format!("Policy \"{name}\" already exists"));
        }
        (*state).trusted_type_policy_names.push(name.clone());
    }

    let policy = sys::JS_NewObject(ctx);
    if sys::js_is_exception(&policy) {
        sys::JS_FreeValue(ctx, handler);
        return policy;
    }
    let name_key = CString::new("name").unwrap();
    sys::JS_SetPropertyStr(ctx, policy, name_key.as_ptr(), new_js_string(ctx, &name));
    let handler_key = CString::new("_createHTML").unwrap();
    sys::JS_SetPropertyStr(ctx, policy, handler_key.as_ptr(), handler);
    let method_name = CString::new("createHTML").unwrap();
    let method = sys::JS_NewCFunction2(
        ctx,
        policy_create_html,
        method_name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, policy, method_name.as_ptr(), method);
    policy
}

/// `trustedTypes.defaultPolicy` — always null here; no default-policy
/// constructor concept exists (a real one comes from a
/// `'allow-duplicates'`-style declaration this engine doesn't parse).
unsafe extern "C" fn default_policy_get(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_null()
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    // The `TrustedHTML` class itself: no exposed constructor (real
    // Trusted Types creates typed values only through policy handlers),
    // just a prototype carrying its string-conversion surface.
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("TrustedHTML").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(trusted_html_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, TRUSTED_HTML_KIND, &def);
    let proto = sys::JS_NewObject(ctx);
    let to_string_name = CString::new("toString").unwrap();
    let to_string = sys::JS_NewCFunction2(
        ctx,
        trusted_html_to_string,
        to_string_name.as_ptr(),
        0,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, proto, to_string_name.as_ptr(), to_string);
    sys::JS_SetClassProto(ctx, class_id, proto);

    let factory = sys::JS_NewObject(ctx);

    let create_name = CString::new("createPolicy").unwrap();
    let create_fn = sys::JS_NewCFunction2(
        ctx,
        create_policy,
        create_name.as_ptr(),
        2,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, factory, create_name.as_ptr(), create_fn);

    define_getter(ctx, factory, "defaultPolicy", default_policy_get);

    let factory_name = CString::new("trustedTypes").unwrap();
    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, factory_name.as_ptr(), factory);
    sys::JS_FreeValue(ctx, global);
}

unsafe fn define_getter(
    ctx: *mut sys::JSContext,
    object: sys::JSValue,
    name: &str,
    getter: unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue) -> sys::JSValue,
) {
    let name_c = CString::new(name).unwrap();
    type Getter =
        unsafe extern "C" fn(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue;
    let f = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(getter),
        name_c.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name_c.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        object,
        atom,
        f,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}
