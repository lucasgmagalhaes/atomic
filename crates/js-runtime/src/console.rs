//! The `console` global: a real leveled message sink, not a no-op. Every
//! `console.log`/`info`/`warn`/`error`/`debug` call formats its arguments
//! the way a devtools console would (strings verbatim, everything else
//! JSON-inspected with a `toString` fallback) and appends a
//! [`ConsoleMessage`] to the context's `HostState.console_messages`
//! buffer, where a host picks it up via [`crate::Context::
//! take_console_messages`] — devtools panels, log files, and this
//! workspace's own IPC protocol all consume the same stream. Registered
//! from [`crate::Context::new`], so `console` exists on every context,
//! DOM-backed or not (a page script calling `console.log` must never be
//! a ReferenceError just because the host didn't attach a DOM).
//!
//! Scope cuts: only the five plain logging methods exist — `assert`,
//! `clear`, `count`, `group*`, `table`, `time`/`timeEnd`, and `trace`
//! are absent (a page calling one gets `TypeError: not a function`, same
//! as any other unimplemented API in this engine); `%s`-style printf
//! substitution in the first argument is not performed (arguments are
//! space-joined, never interpolated); messages have no stack/source
//! location attached.
use std::ffi::{c_int, CStr, CString};

use quickjs_sys as sys;

/// Which method produced a message — the five real levels, ordered by
/// severity for consumers that want to filter (`debug` < `log` < `info` <
/// `warn` < `error`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLevel {
  Debug,
  Log,
  Info,
  Warn,
  Error,
}

impl ConsoleLevel {
  /// The lowercase name every consumer serializes ("error:boom" over
  /// the wire, colored labels in a UI).
  pub fn as_str(&self) -> &'static str {
    match self {
      ConsoleLevel::Debug => "debug",
      ConsoleLevel::Log => "log",
      ConsoleLevel::Info => "info",
      ConsoleLevel::Warn => "warn",
      ConsoleLevel::Error => "error",
    }
  }
}

/// One formatted console line.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsoleMessage {
  pub level: ConsoleLevel,
  pub text: String,
}

/// Upper bound on retained messages per context. A hostile page can call
/// `console.log` in a loop; without a cap that's an unbounded host-side
/// memory leak via a JS-reachable API (the same reasoning behind this
/// crate's other untrusted-input bounds). Once full, the oldest messages
/// are dropped first - a ring, so the most recent output always survives.
const MAX_MESSAGES: usize = 1000;

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
  let console = sys::JS_NewObject(ctx);

  for (name, func) in [
    ("debug", console_debug as sys::JSCFunction),
    ("log", console_log as sys::JSCFunction),
    ("info", console_info as sys::JSCFunction),
    ("warn", console_warn as sys::JSCFunction),
    ("error", console_error as sys::JSCFunction),
  ] {
    let cname = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), 0, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, console, cname.as_ptr(), f);
  }

  let global = sys::JS_GetGlobalObject(ctx);
  let name = CString::new("console").unwrap();
  sys::JS_SetPropertyStr(ctx, global, name.as_ptr(), console);
  sys::JS_FreeValue(ctx, global);
}

unsafe extern "C" fn console_debug(
  ctx: *mut sys::JSContext,
  _this: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  record(ctx, argc, argv, ConsoleLevel::Debug)
}

unsafe extern "C" fn console_log(
  ctx: *mut sys::JSContext,
  _this: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  record(ctx, argc, argv, ConsoleLevel::Log)
}

unsafe extern "C" fn console_info(
  ctx: *mut sys::JSContext,
  _this: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  record(ctx, argc, argv, ConsoleLevel::Info)
}

unsafe extern "C" fn console_warn(
  ctx: *mut sys::JSContext,
  _this: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  record(ctx, argc, argv, ConsoleLevel::Warn)
}

unsafe extern "C" fn console_error(
  ctx: *mut sys::JSContext,
  _this: sys::JSValue,
  argc: c_int,
  argv: *mut sys::JSValue,
) -> sys::JSValue {
  record(ctx, argc, argv, ConsoleLevel::Error)
}

unsafe fn record(
  ctx: *mut sys::JSContext,
  argc: c_int,
  argv: *mut sys::JSValue,
  level: ConsoleLevel,
) -> sys::JSValue {
  let mut parts: Vec<String> = Vec::with_capacity(argc.max(0) as usize);
  for i in 0..argc.max(0) {
    let value = *argv.add(i as usize);
    parts.push(format_value(ctx, value));
  }
  push_message(
    ctx,
    ConsoleMessage {
      level,
      text: parts.join(" "),
    },
  );
  sys::js_undefined()
}

/// Formats one argument the way a devtools console renders it: strings
/// print bare (no quotes), primitives stringify themselves, and anything
/// object-shaped is JSON-inspected - falling back to its own `toString`
/// when JSON can't represent it (functions, `undefined` fields at top
/// level, cyclic structures), and to `[object Object]`-style text when
/// even that fails or throws.
unsafe fn format_value(ctx: *mut sys::JSContext, value: sys::JSValue) -> String {
  if value.tag == sys::JS_TAG_STRING {
    return read_string_or_empty(ctx, value);
  }
  if value.tag == sys::JS_TAG_UNDEFINED {
    return "undefined".to_string();
  }
  if value.tag == sys::JS_TAG_NULL {
    return "null".to_string();
  }
  if value.tag != sys::JS_TAG_OBJECT {
    // Numbers, booleans, bigints, symbols: their own stringification
    // is exactly what a console shows.
    return read_string_or_empty(ctx, value);
  }

  // Object-shaped: try JSON first (arrays render as `[1,2]`, plain
  // objects as `{"key":"value"}` - far more readable than
  // `[object Object]`).
  let json = sys::JS_JSONStringify(ctx, value, sys::js_undefined(), sys::js_undefined());
  if !sys::js_is_exception(&json) && json.tag != sys::JS_TAG_UNDEFINED {
    let text = read_string_or_empty(ctx, json);
    sys::JS_FreeValue(ctx, json);
    return text;
  }
  if sys::js_is_exception(&json) {
    // A throwing `toJSON` leaves an exception pending; clear it -
    // formatting must never leak an exception into the caller's
    // subsequent operations. The returned value owns the error
    // object, so it must be dropped, not just discarded.
    let thrown = sys::JS_GetException(ctx);
    sys::JS_FreeValue(ctx, thrown);
  } else {
    sys::JS_FreeValue(ctx, json);
  }

  // Fall back to toString (functions show their source, class instances
  // their inherited toString).
  let text = read_string_or_empty(ctx, value);
  if text.is_empty() {
    "[unprintable]".to_string()
  } else {
    text
  }
}

unsafe fn read_string_or_empty(ctx: *mut sys::JSContext, value: sys::JSValue) -> String {
  let mut len: usize = 0;
  let ptr = sys::JS_ToCStringLen2(ctx, &mut len, value, false);
  if ptr.is_null() {
    // A failed conversion (e.g. a throwing toString) may leave an
    // exception pending - clear it, same policy as format_value's
    // JSON path. The returned value owns the error object.
    if sys::JS_HasException(ctx) {
      let thrown = sys::JS_GetException(ctx);
      sys::JS_FreeValue(ctx, thrown);
    }
    return String::new();
  }
  let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
  sys::JS_FreeCString(ctx, ptr);
  s
}

pub(crate) unsafe fn push_message(ctx: *mut sys::JSContext, message: ConsoleMessage) {
  let state = crate::host_state::get(ctx);
  if state.is_null() {
    return;
  }
  let messages = &mut (*state).console_messages;
  messages.push(message);
  let excess = messages.len().saturating_sub(MAX_MESSAGES);
  if excess > 0 {
    messages.drain(0..excess);
  }
}
