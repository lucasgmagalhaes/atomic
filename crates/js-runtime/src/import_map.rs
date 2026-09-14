//! Real import maps (`ROADMAP.md` item 19's remaining sub-item):
//! `Context::set_import_map(json)` parses a real `{"imports": {"bare":
//! "url", "prefix/": "url/"}}` object via `JS_ParseJSON` (the dedicated
//! C API `CLAUDE.md`'s own gotcha calls for — never a re-entrant
//! `JS_Eval` from inside a native callback) and `JS_GetOwnPropertyNames`
//! (same real enumeration `value_bridge.rs`'s `js_object_to_value`
//! already uses).
//!
//! Stored in a thread-local keyed by `ctx as usize`
//! (`module_loader.rs`'s own `CACHES` shape), not `HostState` — an
//! import map is module-loading state, not page/DOM state, and
//! `Context::new` (no `HostState` at all - only `Context::with_dom`
//! builds one) still needs a working import map for a classic-script-
//! only realm that still uses dynamic `import()`.
//!
//! [`resolve`] is the read side `module_loader::resolve_specifier` calls
//! for a bare specifier (no scheme, not `./`/`../`/`/`): an exact match
//! wins outright; otherwise the longest `"prefix/"`-keyed entry that's a
//! real prefix of the specifier wins, with the matched prefix replaced
//! by its mapped value and the remainder appended — real spec's own
//! prefix-mapping rule (e.g. `"@app/": "/src/app/"` maps
//! `"@app/utils.mjs"` to `"/src/app/utils.mjs"`). No match: `None`, same
//! "can't resolve without one" outcome as before this module existed.
//!
//! Scope cuts: no `scopes` (real spec's per-URL-prefix override maps) —
//! one flat `imports` map only. No "one import map, must precede every
//! module script" enforcement (real spec forbids a second one after
//! module execution starts) — [`set_import_map`] can be called any
//! number of times, last write wins, same "host controls timing, no
//! window into a concurrent execution" trust boundary every other
//! host-facing setter here has.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;

use quickjs_sys as sys;

thread_local! {
    static MAPS: RefCell<HashMap<usize, HashMap<String, String>>> = RefCell::new(HashMap::new());
}

/// Real `specifier -> resolved URL` lookup for `ctx`'s own import map —
/// see this module's own doc for the exact-match-then-longest-prefix
/// rule. `None` if `ctx` never called [`set_import_map`], or the
/// specifier matches nothing in it.
pub(crate) fn resolve(ctx: *mut sys::JSContext, specifier: &str) -> Option<String> {
    MAPS.with(|cell| {
        let maps = cell.borrow();
        let map = maps.get(&(ctx as usize))?;
        if let Some(exact) = map.get(specifier) {
            return Some(exact.clone());
        }
        map.iter()
            .filter(|(key, _)| key.ends_with('/') && specifier.starts_with(key.as_str()))
            .max_by_key(|(key, _)| key.len())
            .map(|(key, value)| format!("{value}{}", &specifier[key.len()..]))
    })
}

/// Evicts `ctx`'s entry — called from `Context::drop` alongside
/// `module_loader::cleanup`, same "own state, own eviction call" shape.
pub(crate) fn cleanup(ctx: *mut sys::JSContext) {
    MAPS.with(|cell| {
        cell.borrow_mut().remove(&(ctx as usize));
    });
}

/// Real thrown-exception -> `String`, same pattern `context/eval.rs`'s
/// own `eval`/`eval_module` already use (`JS_GetException`/
/// `JS_ToCStringLen2`, each intermediate `JSValue` explicitly freed —
/// see `CLAUDE.md`'s own gotcha on why leaving these to fall out of
/// scope silently leaks under `JS_GetException`/`JS_GetGlobalObject`'s
/// "returns an owned reference" contract).
unsafe fn exception_message(ctx: *mut sys::JSContext) -> String {
    let exception = sys::JS_GetException(ctx);
    if exception.tag == sys::JS_TAG_NULL {
        return "invalid import map JSON".to_string();
    }
    let mut len: usize = 0;
    let c_str_ptr = sys::JS_ToCStringLen2(ctx, &mut len, exception, false);
    sys::JS_FreeValue(ctx, exception);
    if c_str_ptr.is_null() {
        return "invalid import map JSON".to_string();
    }
    let text = CStr::from_ptr(c_str_ptr).to_string_lossy().into_owned();
    sys::JS_FreeCString(ctx, c_str_ptr);
    text
}

/// Parses `json` as a real import map and replaces `ctx`'s import map
/// wholesale (last write wins — see this module's own doc). Real spec:
/// every mapped value is itself resolved as a URL against `base_url`
/// (the document's own URL, where the real `<script type="importmap">`
/// lives) *once*, at parse time — not re-resolved on every specifier
/// lookup — so a root-relative value like `"/src/app/"` becomes a real
/// absolute URL immediately (an unparseable individual value is silently
/// dropped from the map rather than failing the whole parse, matching
/// real spec's own per-entry error tolerance). `Err` on malformed JSON
/// or a real thrown exception from `JS_ParseJSON`; a missing/non-object
/// `imports` key degrades to an empty map rather than erroring (real
/// spec: an import map with no `imports` key is valid, just contributes
/// nothing).
pub(crate) unsafe fn set_import_map(
    ctx: *mut sys::JSContext,
    base_url: &str,
    json: &str,
) -> Result<(), String> {
    let filename = std::ffi::CString::new("<import map>").unwrap();
    // `JS_ParseJSON`'s `buf`/`buf_len` pair still needs `buf[buf_len]` to
    // be a real, readable `'\0'` byte (same lexer-sentinel convention
    // `JS_Eval`'s own `code_c.as_ptr()`/`.as_bytes().len()` calls
    // elsewhere in this crate satisfy by construction via `CString`) - a
    // plain `&str`'s buffer has no such guarantee, and reading one byte
    // past it is real undefined behavior (confirmed while developing
    // this: manifested as a spurious "unexpected data at the end"
    // `SyntaxError` from whatever garbage byte happened to follow the
    // string in memory).
    let json_c = match std::ffi::CString::new(json) {
        Ok(c) => c,
        Err(_) => return Err("import map JSON must not contain NUL bytes".to_string()),
    };
    let parsed = sys::JS_ParseJSON(ctx, json_c.as_ptr(), json.len(), filename.as_ptr());
    if sys::js_is_exception(&parsed) {
        return Err(exception_message(ctx));
    }

    let base = url::Url::parse(base_url).ok();

    let imports_key = std::ffi::CString::new("imports").unwrap();
    let imports_val = sys::JS_GetPropertyStr(ctx, parsed, imports_key.as_ptr());
    let mut map = HashMap::new();
    if imports_val.tag == sys::JS_TAG_OBJECT {
        let mut tab: *mut sys::JSPropertyEnum = std::ptr::null_mut();
        let mut len: u32 = 0;
        let rc = sys::JS_GetOwnPropertyNames(
            ctx,
            &mut tab,
            &mut len,
            imports_val,
            sys::JS_GPN_STRING_ENUM,
        );
        if rc >= 0 && !tab.is_null() {
            let entries = std::slice::from_raw_parts(tab, len as usize);
            for entry in entries {
                let mut name_len: usize = 0;
                let name_ptr = sys::JS_AtomToCStringLen(ctx, &mut name_len, entry.atom);
                if name_ptr.is_null() {
                    continue;
                }
                let key_bytes = std::slice::from_raw_parts(name_ptr as *const u8, name_len);
                let key = String::from_utf8_lossy(key_bytes).into_owned();
                sys::JS_FreeCString(ctx, name_ptr);

                let prop_val = sys::JS_GetProperty(ctx, imports_val, entry.atom);
                if prop_val.tag == sys::JS_TAG_STRING {
                    let mut val_len: usize = 0;
                    let val_ptr = sys::JS_ToCStringLen2(ctx, &mut val_len, prop_val, false);
                    if !val_ptr.is_null() {
                        let val_bytes = std::slice::from_raw_parts(val_ptr as *const u8, val_len);
                        let raw_value = String::from_utf8_lossy(val_bytes).into_owned();
                        let resolved = base
                            .as_ref()
                            .and_then(|b| b.join(&raw_value).ok())
                            .map(|u| u.to_string());
                        if let Some(resolved) = resolved {
                            map.insert(key, resolved);
                        }
                        sys::JS_FreeCString(ctx, val_ptr);
                    }
                }
                sys::JS_FreeValue(ctx, prop_val);
            }
            sys::JS_FreePropertyEnum(ctx, tab, len);
        }
    }
    sys::JS_FreeValue(ctx, imports_val);
    sys::JS_FreeValue(ctx, parsed);

    MAPS.with(|cell| {
        cell.borrow_mut().insert(ctx as usize, map);
    });
    Ok(())
}
