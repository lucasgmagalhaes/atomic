//! The stored representation (`FormDataInner`/`Entry`/`EntryValue`), its
//! size bounds, and the opaque-data plumbing (`form_data_opaque`,
//! `form_data_finalizer`, `with_entries`) every other submodule reaches
//! through.

use quickjs_sys as sys;

use super::util::FORM_DATA_CLASS_KIND;

pub(super) const MAX_ENTRIES: usize = 1000;
pub(super) const MAX_NAME_LENGTH: usize = 1024;
pub(super) const MAX_VALUE_LENGTH: usize = 64 * 1024;
pub(super) const MAX_FILENAME_LENGTH: usize = 255;

pub(super) enum EntryValue {
  Text(String),
  File {
    bytes: Vec<u8>,
    mime: String,
    filename: String,
  },
}

pub(super) struct Entry {
  pub(super) name: String,
  pub(super) value: EntryValue,
}

pub(super) struct FormDataInner {
  pub(super) entries: Vec<Entry>,
}

/// Appends `(name, value)` unless a bound is hit (silently dropped, same
/// convention `append_part` in `blob.rs` uses for unsupported parts).
pub(super) fn push_entry(inner: &mut FormDataInner, name: String, value: EntryValue) -> bool {
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

pub(super) unsafe fn form_data_opaque(
  rt: *mut sys::JSRuntime,
  this_val: sys::JSValue,
) -> *mut FormDataInner {
  let class_id = crate::class_registry::class_id_for(rt, FORM_DATA_CLASS_KIND);
  if class_id == 0 {
    return std::ptr::null_mut();
  }
  sys::JS_GetOpaque(this_val, class_id) as *mut FormDataInner
}

pub(super) unsafe extern "C" fn form_data_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
  let ptr = form_data_opaque(rt, val);
  if !ptr.is_null() {
    drop(Box::from_raw(ptr));
  }
}

pub(super) unsafe fn with_entries<R>(
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
