//! `FormData` — an ordered name→value list backed by a real `Vec<Entry>`
//! (not a placeholder object), with the full per-spec method surface:
//! `append`/`delete`/`get`/`getAll`/`has`/`set`/`forEach`, plus
//! `entries()`/`keys()`/`values()`.
//!
//! Deviations from the real API (each mirrors an existing documented cut
//! elsewhere in this crate):
//!
//! * `new FormData(form)` populates from the passed `<form>`'s controls —
//!   named `input`s (`checkbox`/`radio` only when checked, everything else
//!   via the generic `.value`, so a `<textarea>` falls back to its text
//!   content exactly like `.value` does) and named `<select>`s with an
//!   explicitly-selected `<option>` (effective value = its `value`
//!   attribute or text content). Buttons are never included (real spec
//!   includes only the submitter, which doesn't exist in this engine), and
//!   file inputs degrade to their `value` attribute (no filesystem).
//!   Without a live DOM behind the context (plain [`Context::new`]) the
//!   constructor degrades to an empty instance rather than throwing.
//! * Stored file values are snapshots: appending a `Blob`/`File` copies its
//!   bytes immediately (the real spec does this too); `get()` returns a
//!   fresh genuine `Blob` instance per call carrying `name`/`lastModified`
//!   own properties when a filename was given.
//! * `entries()`/`keys()`/`values()` return plain arrays of pairs/values,
//!   not iterator objects — QuickJS's well-known-symbol lookup isn't bound
//!   in `quickjs-sys` yet, same reason `attributes` iteration uses arrays.
//!   Use `forEach` for callback-style walking.
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

const FORM_DATA_CLASS_KIND: &str = "FormData";
const MAX_ENTRIES: usize = 1000;
const MAX_NAME_LENGTH: usize = 1024;
const MAX_VALUE_LENGTH: usize = 64 * 1024;
const MAX_FILENAME_LENGTH: usize = 255;

enum EntryValue {
    Text(String),
    File {
        bytes: Vec<u8>,
        mime: String,
        filename: String,
    },
}

struct Entry {
    name: String,
    value: EntryValue,
}

struct FormDataInner {
    entries: Vec<Entry>,
}

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
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

unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe fn throw_type_error(ctx: *mut sys::JSContext, message: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_js_string(ctx, message))
}

unsafe fn form_data_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut FormDataInner {
    let class_id = crate::class_registry::class_id_for(rt, FORM_DATA_CLASS_KIND);
    if class_id == 0 {
        return std::ptr::null_mut();
    }
    sys::JS_GetOpaque(this_val, class_id) as *mut FormDataInner
}

unsafe extern "C" fn form_data_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = form_data_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

/// Resolves a fresh `Blob` instance wrapping `bytes`/`mime` via the global
/// `Blob` constructor's prototype — used by `get()`/`getAll()` so stored
/// file values come back as real Blobs (with `slice`/`text`/`arrayBuffer`
/// all working), not inert stubs.
pub(crate) unsafe fn blob_from_bytes(
    ctx: *mut sys::JSContext,
    bytes: Vec<u8>,
    mime: String,
) -> sys::JSValue {
    let class_id =
        crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), crate::blob::BLOB_CLASS_KIND);
    if class_id == 0 {
        return sys::js_null();
    }
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    let global = sys::JS_GetGlobalObject(ctx);
    let ctor_name = CString::new("Blob").unwrap();
    let ctor = sys::JS_GetPropertyStr(ctx, global, ctor_name.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let proto_name = CString::new("prototype").unwrap();
    let proto = sys::JS_GetPropertyStr(ctx, ctor, proto_name.as_ptr());
    sys::JS_FreeValue(ctx, ctor);
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);
    sys::JS_SetOpaque(
        obj,
        Box::into_raw(Box::new(crate::blob::BlobInner { bytes, mime })) as *mut c_void,
    );
    obj
}

/// Appends `(name, value)` unless a bound is hit (silently dropped, same
/// convention `append_part` in `blob.rs` uses for unsupported parts).
fn push_entry(inner: &mut FormDataInner, name: String, value: EntryValue) -> bool {
    if inner.entries.len() >= MAX_ENTRIES || name.is_empty() || name.len() > MAX_NAME_LENGTH {
        return false;
    }
    if let EntryValue::Text(text) = &value {
        if text.len() > MAX_VALUE_LENGTH {
            return false;
        }
    }
    if let EntryValue::File { filename, .. } = &value {
        if filename.len() > MAX_FILENAME_LENGTH {
            return false;
        }
    }
    inner.entries.push(Entry { name, value });
    true
}

/// Reads one append/set value argument: a string → text entry; a Blob/File
/// instance → snapshot file entry (with `filename` or the File's own
/// `name`). Anything else returns `None`.
unsafe fn read_entry_value(
    ctx: *mut sys::JSContext,
    value: sys::JSValue,
    filename_arg: Option<sys::JSValue>,
) -> Option<EntryValue> {
    if value.tag == sys::JS_TAG_STRING {
        let text = read_js_string(ctx, value)?;
        return Some(EntryValue::Text(text));
    }
    if value.tag == sys::JS_TAG_OBJECT {
        let rt = sys::JS_GetRuntime(ctx);
        let blob_class_id = crate::class_registry::class_id_for(rt, crate::blob::BLOB_CLASS_KIND);
        if blob_class_id != 0 {
            let blob_ptr = sys::JS_GetOpaque(value, blob_class_id) as *mut crate::blob::BlobInner;
            if !blob_ptr.is_null() {
                // A File's own `name` is the default filename when none
                // was passed explicitly.
                let own_name = {
                    let name_c = CString::new("name").unwrap();
                    let v = sys::JS_GetPropertyStr(ctx, value, name_c.as_ptr());
                    let s = read_js_string(ctx, v).unwrap_or_default();
                    sys::JS_FreeValue(ctx, v);
                    s
                };
                let filename = match filename_arg {
                    Some(f) => read_js_string(ctx, f).unwrap_or(own_name),
                    None => own_name,
                };
                return Some(EntryValue::File {
                    bytes: (*blob_ptr).bytes.clone(),
                    mime: (*blob_ptr).mime.clone(),
                    filename,
                });
            }
        }
    }
    None
}

unsafe fn set_str_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: &str) {
    let name = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), new_js_string(ctx, val));
}

/// Builds the JS return value for one stored entry: text → string; file →
/// a real `Blob` with `name`/`lastModified` own props when a filename was
/// recorded (mirrors `file_constructor` in `blob.rs`).
unsafe fn entry_to_js(ctx: *mut sys::JSContext, entry: &Entry) -> sys::JSValue {
    match &entry.value {
        EntryValue::Text(text) => new_js_string(ctx, text),
        EntryValue::File {
            bytes,
            mime,
            filename,
        } => {
            let blob = blob_from_bytes(ctx, bytes.clone(), mime.clone());
            if !filename.is_empty() && blob.tag == sys::JS_TAG_OBJECT {
                set_str_prop(ctx, blob, "name", filename);
                let lm = CString::new("lastModified").unwrap();
                sys::JS_SetPropertyStr(ctx, blob, lm.as_ptr(), sys::js_float64(0.0));
            }
            blob
        }
    }
}

unsafe extern "C" fn form_data_constructor(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let mut entries: Vec<Entry> = Vec::new();
    if argc >= 1 && (*argv).tag == sys::JS_TAG_OBJECT {
        populate_from_form(ctx, *argv, &mut entries);
    }

    // Same explicit-prototype resolution `blob.rs` uses: `new_target`'s
    // `.prototype` wins, else `globalThis.FormData.prototype` — the
    // per-context implicit default has proven unreliable under concurrent
    // Runtime construction (see `resolve_prototype` there).
    let class_id =
        crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), FORM_DATA_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    let mut proto = if new_target.tag != sys::JS_TAG_UNDEFINED {
        let proto_name = CString::new("prototype").unwrap();
        sys::JS_GetPropertyStr(ctx, new_target, proto_name.as_ptr())
    } else {
        sys::js_undefined()
    };
    if proto.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, proto);
        let global = sys::JS_GetGlobalObject(ctx);
        let ctor_name = CString::new("FormData").unwrap();
        let ctor = sys::JS_GetPropertyStr(ctx, global, ctor_name.as_ptr());
        sys::JS_FreeValue(ctx, global);
        let proto_name = CString::new("prototype").unwrap();
        proto = sys::JS_GetPropertyStr(ctx, ctor, proto_name.as_ptr());
        sys::JS_FreeValue(ctx, ctor);
    }
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);
    sys::JS_SetOpaque(
        obj,
        Box::into_raw(Box::new(FormDataInner { entries })) as *mut c_void,
    );
    obj
}

/// Populates `entries` from a `<form>` element node — controls walked in
/// document order, rules documented on the module. Degrades to no-op when
/// there's no DOM behind the context.
unsafe fn populate_from_form(
    ctx: *mut sys::JSContext,
    form: sys::JSValue,
    entries: &mut Vec<Entry>,
) {
    let Some(id) = crate::dom_bindings::node_id(ctx, form) else {
        return;
    };
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return;
    }
    let dom = std::ptr::addr_of_mut!((*state).dom);
    let controls = descendants_matching_tags(&*dom, id, &["input", "select", "textarea"]);
    for control in controls {
        let name = match (*dom).attribute(control, "name") {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };
        if (*dom).attribute(control, "disabled").is_some() {
            continue;
        }
        let tag = match (*dom).get(control).map(|n| &n.data) {
            Some(dom::NodeData::Element { tag, .. }) => tag.as_str(),
            _ => continue,
        };
        let value = match tag {
            "input" => {
                let input_type = (*dom)
                    .attribute(control, "type")
                    .unwrap_or("text")
                    .to_ascii_lowercase();
                if input_type == "checkbox" || input_type == "radio" {
                    if (*dom).attribute(control, "checked").is_none() {
                        continue;
                    }
                    (*dom)
                        .attribute(control, "value")
                        .unwrap_or("on")
                        .to_string()
                } else {
                    (*dom).value(control)
                }
            }
            "select" => {
                let Some(selected) = selected_option(&*dom, control) else {
                    continue;
                };
                option_effective_value(&*dom, selected)
            }
            // textarea falls through to Dom::value's text-content fallback
            _ => (*dom).value(control),
        };
        entries.push(Entry {
            name,
            value: EntryValue::Text(value),
        });
    }
}

/// All element descendants of `start` whose tag is in `tags`, document
/// order (same iterative-DFS shape as `dom_bindings::matching_by_tag`,
/// with that function's visit/result limits).
fn descendants_matching_tags(
    dom: &dom::Dom,
    start: dom::NodeId,
    tags: &[&str],
) -> Vec<dom::NodeId> {
    const MAX_VISITS: usize = 4096;
    const MAX_RESULTS: usize = 4096;
    let mut result = Vec::new();
    let mut stack = vec![start];
    let mut visits = 0usize;
    while let Some(id) = stack.pop() {
        visits += 1;
        if visits > MAX_VISITS || result.len() > MAX_RESULTS {
            break;
        }
        if let Some(node) = dom.get(id) {
            if id != start {
                if let dom::NodeData::Element { tag, .. } = &node.data {
                    if tags.contains(&tag.as_str()) {
                        result.push(id);
                    }
                }
            }
            stack.extend(node.children.iter().rev().copied());
        }
    }
    result
}

/// The first explicitly-selected `<option>` under `select`, mirroring
/// `select_value_get`'s documented deviation (nothing implicitly picked).
fn selected_option(dom: &dom::Dom, select: dom::NodeId) -> Option<dom::NodeId> {
    descendants_matching_tags(dom, select, &["option"])
        .into_iter()
        .find(|option| dom.attribute(*option, "selected").is_some())
}

/// An option's effective value: its `value` attribute when present,
/// otherwise its text content (same rule `dom_bindings` uses).
fn option_effective_value(dom: &dom::Dom, option: dom::NodeId) -> String {
    match dom.attribute(option, "value") {
        Some(v) => v.to_string(),
        None => dom.text_content(option),
    }
}

unsafe fn with_entries<R>(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    on_missing: impl FnOnce() -> R,
    body: impl FnOnce(&mut FormDataInner) -> R,
) -> R {
    let ptr = form_data_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return on_missing();
    }
    body(&mut *ptr)
}

unsafe extern "C" fn form_data_append(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "append expects a name and a value");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "append name must be a string");
    };
    let value_arg = *argv.add(1);
    let filename_arg = if argc >= 3 { Some(*argv.add(2)) } else { None };
    let Some(value) = read_entry_value(ctx, value_arg, filename_arg) else {
        // Non-string non-Blob values are silently skipped (documented
        // deviation, same shape as Blob constructor parts).
        return sys::js_undefined();
    };
    with_entries(
        ctx,
        this_val,
        || (),
        |inner| {
            push_entry(inner, name, value);
        },
    );
    sys::js_undefined()
}

unsafe extern "C" fn form_data_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "set expects a name and a value");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "set name must be a string");
    };
    let value_arg = *argv.add(1);
    let filename_arg = if argc >= 3 { Some(*argv.add(2)) } else { None };
    let Some(new_value) = read_entry_value(ctx, value_arg, filename_arg) else {
        return sys::js_undefined();
    };
    with_entries(
        ctx,
        this_val,
        || (),
        |inner| {
            inner.entries.retain(|entry| entry.name != name);
            push_entry(inner, name, new_value);
        },
    );
    sys::js_undefined()
}

unsafe extern "C" fn form_data_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "get expects a name");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "get name must be a string");
    };
    with_entries(
        ctx,
        this_val,
        || sys::js_null(),
        |inner| match inner.entries.iter().find(|entry| entry.name == name) {
            Some(entry) => entry_to_js(ctx, entry),
            None => sys::js_null(),
        },
    )
}

unsafe extern "C" fn form_data_get_all(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "getAll expects a name");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "getAll name must be a string");
    };
    with_entries(
        ctx,
        this_val,
        || sys::JS_NewArray(ctx),
        |inner| {
            let array = sys::JS_NewArray(ctx);
            let mut index = 0u32;
            for entry in inner.entries.iter().filter(|e| e.name == name) {
                sys::JS_SetPropertyUint32(ctx, array, index, entry_to_js(ctx, entry));
                index += 1;
            }
            array
        },
    )
}

unsafe extern "C" fn form_data_has(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "has expects a name");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "has name must be a string");
    };
    with_entries(
        ctx,
        this_val,
        || sys::js_bool(false),
        |inner| sys::js_bool(inner.entries.iter().any(|entry| entry.name == name)),
    )
}

unsafe extern "C" fn form_data_delete(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "delete expects a name");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "delete name must be a string");
    };
    with_entries(
        ctx,
        this_val,
        || (),
        |inner| inner.entries.retain(|entry| entry.name != name),
    );
    sys::js_undefined()
}

/// Shared tail of `entries`/`keys`/`values`: builds an array where each
/// element is either the bare value (`values`), the bare name (`keys`), or
/// a two-element `[name, value]` pair (`entries`).
unsafe fn collect_array(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    mode: CollectMode,
) -> sys::JSValue {
    with_entries(
        ctx,
        this_val,
        || sys::JS_NewArray(ctx),
        |inner| {
            let array = sys::JS_NewArray(ctx);
            let mut index = 0u32;
            for entry in inner.entries.iter() {
                let item = match mode {
                    CollectMode::Keys => new_js_string(ctx, &entry.name),
                    CollectMode::Values => entry_to_js(ctx, entry),
                    CollectMode::Entries => {
                        let pair = sys::JS_NewArray(ctx);
                        sys::JS_SetPropertyUint32(ctx, pair, 0, new_js_string(ctx, &entry.name));
                        sys::JS_SetPropertyUint32(ctx, pair, 1, entry_to_js(ctx, entry));
                        pair
                    }
                };
                sys::JS_SetPropertyUint32(ctx, array, index, item);
                index += 1;
            }
            array
        },
    )
}

#[derive(Clone, Copy)]
enum CollectMode {
    Entries,
    Keys,
    Values,
}

unsafe extern "C" fn form_data_entries(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    collect_array(ctx, this_val, CollectMode::Entries)
}

unsafe extern "C" fn form_data_keys(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    collect_array(ctx, this_val, CollectMode::Keys)
}

unsafe extern "C" fn form_data_values(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    collect_array(ctx, this_val, CollectMode::Values)
}

unsafe extern "C" fn form_data_for_each(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "forEach expects a callback");
    }
    let callback = *argv;
    if callback.tag != sys::JS_TAG_OBJECT {
        return throw_type_error(ctx, "forEach callback must be callable");
    }
    with_entries(
        ctx,
        this_val,
        || (),
        |inner| {
            // Snapshot names first: a callback calling delete()/append()
            // mid-walk must not see shifting indices (spec iterates a live
            // list, but snapshotting is strictly safer and simpler).
            let snapshot: Vec<(String, EntryValue)> = inner
                .entries
                .iter()
                .map(|entry| match &entry.value {
                    EntryValue::Text(text) => (entry.name.clone(), EntryValue::Text(text.clone())),
                    EntryValue::File {
                        bytes,
                        mime,
                        filename,
                    } => (
                        entry.name.clone(),
                        EntryValue::File {
                            bytes: bytes.clone(),
                            mime: mime.clone(),
                            filename: filename.clone(),
                        },
                    ),
                })
                .collect();
            for (name, value) in snapshot {
                let synthetic = Entry { name, value };
                let mut args = [
                    entry_to_js(ctx, &synthetic),
                    new_js_string(ctx, &synthetic.name),
                    sys::JS_DupValue(ctx, this_val),
                ];
                let result = sys::JS_Call(ctx, callback, sys::js_undefined(), 3, args.as_mut_ptr());
                for arg in args {
                    sys::JS_FreeValue(ctx, arg);
                }
                if sys::js_is_exception(&result) {
                    // Propagate: leave the exception pending, stop walking.
                    return;
                }
                sys::JS_FreeValue(ctx, result);
            }
        },
    );
    sys::js_undefined()
}

/// Registers the `FormData` global. Safe to call without a DOM behind the
/// context — the `new FormData(form)` population path degrades to an empty
/// instance there (see module docs).
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("FormData").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(form_data_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, FORM_DATA_CLASS_KIND, &def);

    let proto = sys::JS_NewObject(ctx);
    for (name, func, length) in [
        ("append", form_data_append as sys::JSCFunction, 2),
        ("set", form_data_set as sys::JSCFunction, 2),
        ("get", form_data_get as sys::JSCFunction, 1),
        ("getAll", form_data_get_all as sys::JSCFunction, 1),
        ("has", form_data_has as sys::JSCFunction, 1),
        ("delete", form_data_delete as sys::JSCFunction, 1),
        ("entries", form_data_entries as sys::JSCFunction, 0),
        ("keys", form_data_keys as sys::JSCFunction, 0),
        ("values", form_data_values as sys::JSCFunction, 0),
        ("forEach", form_data_for_each as sys::JSCFunction, 1),
    ] {
        let cname = CString::new(name).unwrap();
        let f = sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, proto, cname.as_ptr(), f);
    }

    let ctor_name = CString::new("FormData").unwrap();
    let ctor_fn = sys::JS_NewCFunction2(
        ctx,
        form_data_constructor,
        ctor_name.as_ptr(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        ctor_fn,
        proto_name.as_ptr(),
        sys::JS_DupValue(ctx, proto),
    );
    sys::JS_SetClassProto(ctx, class_id, proto);

    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, ctor_name.as_ptr(), ctor_fn);
    sys::JS_FreeValue(ctx, global);
}
