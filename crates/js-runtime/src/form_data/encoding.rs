//! `multipart/form-data` encoding (the wire format `fetch`/`XMLHttpRequest`
//! send a `FormData` body as) plus the reverse direction — turning a stored
//! [`super::types::Entry`] back into a JS value (`get`/`getAll`/`entries`/...
//! all funnel through [`entry_to_js`]).

use std::ffi::CString;
use std::os::raw::c_void;

use quickjs_sys as sys;

use super::types::{Entry, EntryValue};
use super::util::{new_js_string, read_js_string};

static NEXT_BOUNDARY_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The `multipart/form-data` serialization of one stored entry, per the
/// HTML spec's encoding algorithm (same shape browsers send for
/// `new FormData(form)` POSTs).
fn encode_entry(out: &mut Vec<u8>, boundary: &str, entry: &Entry) {
  let safe_name = entry.name.replace('"', "%22");
  match &entry.value {
    EntryValue::Text(text) => {
      out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
      out.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{safe_name}\"\r\n\r\n").as_bytes(),
      );
      out.extend_from_slice(text.as_bytes());
      out.extend_from_slice(b"\r\n");
    }
    EntryValue::File {
      bytes,
      mime,
      filename,
    } => {
      // An empty filename serializes as `filename=""` per spec
      // (browsers also always include a filename part for file
      // entries); a "file" with no name at all becomes "blob".
      let name = if filename.is_empty() {
        "blob"
      } else {
        filename
      };
      let safe_filename = name.replace('"', "%22");
      let mime = if mime.is_empty() {
        "application/octet-stream"
      } else {
        mime
      };
      out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
      out.extend_from_slice(
        format!(
          "Content-Disposition: form-data; name=\"{safe_name}\"; filename=\"{safe_filename}\"\r\n"
        )
        .as_bytes(),
      );
      out.extend_from_slice(format!("Content-Type: {mime}\r\n\r\n").as_bytes());
      out.extend_from_slice(bytes);
      out.extend_from_slice(b"\r\n");
    }
  }
}

/// What `fetch`/`XMLHttpRequest` need to send a FormData body: the encoded
/// bytes plus the exact `Content-Type` header value (boundary included).
pub(crate) struct SerializedForm {
  pub content_type: String,
  pub body: Vec<u8>,
}

/// Serializes this FormData instance to `multipart/form-data` bytes.
/// Returns `None` when `value` isn't a FormData instance (or no class is
/// registered yet) — callers treat that as "not a form body".
pub(crate) unsafe fn serialize(
  ctx: *mut sys::JSContext,
  value: sys::JSValue,
) -> Option<SerializedForm> {
  if value.tag != sys::JS_TAG_OBJECT {
    return None;
  }
  let class_id =
    crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), super::util::FORM_DATA_CLASS_KIND);
  if class_id == 0 {
    return None;
  }
  let ptr = sys::JS_GetOpaque(value, class_id) as *mut super::types::FormDataInner;
  if ptr.is_null() {
    return None;
  }
  let id = NEXT_BOUNDARY_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let boundary = format!("----nimbleFormData{id:016x}");
  let mut body = Vec::new();
  for entry in &(*ptr).entries {
    encode_entry(&mut body, &boundary, entry);
  }
  body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
  Some(SerializedForm {
    content_type: format!("multipart/form-data; boundary={boundary}"),
    body,
  })
}

/// Resolves a fresh `Blob` instance wrapping `bytes`/`mime` via the global
/// `Blob` constructor's prototype — used by `get()`/`getAll()` so stored
/// file values come back as real Blobs (with `slice`/`text`/`arrayBuffer`
/// all working), not inert stubs.
pub(super) unsafe fn blob_from_bytes(
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

unsafe fn set_str_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: &str) {
  let name = CString::new(key).unwrap();
  sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), new_js_string(ctx, val));
}

/// Builds the JS return value for one stored entry: text → string; file →
/// a real `Blob` with `name`/`lastModified` own props when a filename was
/// recorded (mirrors `file_constructor` in `blob.rs`).
pub(super) unsafe fn entry_to_js(ctx: *mut sys::JSContext, entry: &Entry) -> sys::JSValue {
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

/// Reads one append/set value argument: a string → text entry; a Blob/File
/// instance → snapshot file entry (with `filename` or the File's own
/// `name`). Anything else returns `None`.
pub(super) unsafe fn read_entry_value(
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
