//! `FormData.prototype`'s per-spec method surface: `append`/`set`/`get`/
//! `getAll`/`has`/`delete`/`entries`/`keys`/`values`/`forEach`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::encoding::{entry_to_js, read_entry_value};
use super::types::{push_entry, with_entries, Entry, EntryValue};
use super::util::{new_js_string, read_js_string, throw_type_error};

pub(super) unsafe extern "C" fn form_data_append(
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

pub(super) unsafe extern "C" fn form_data_set(
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

pub(super) unsafe extern "C" fn form_data_get(
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

pub(super) unsafe extern "C" fn form_data_get_all(
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

pub(super) unsafe extern "C" fn form_data_has(
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

pub(super) unsafe extern "C" fn form_data_delete(
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

#[derive(Clone, Copy)]
enum CollectMode {
  Entries,
  Keys,
  Values,
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

pub(super) unsafe extern "C" fn form_data_entries(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  collect_array(ctx, this_val, CollectMode::Entries)
}

pub(super) unsafe extern "C" fn form_data_keys(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  collect_array(ctx, this_val, CollectMode::Keys)
}

pub(super) unsafe extern "C" fn form_data_values(
  ctx: *mut sys::JSContext,
  this_val: sys::JSValue,
  _argc: c_int,
  _argv: *mut sys::JSValue,
) -> sys::JSValue {
  collect_array(ctx, this_val, CollectMode::Values)
}

pub(super) unsafe extern "C" fn form_data_for_each(
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
