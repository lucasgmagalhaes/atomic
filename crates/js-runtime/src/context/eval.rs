//! Script evaluation + uncaught-error reporting — split out from
//! `context/mod.rs`.

use std::ffi::{CStr, CString};

use quickjs_sys as sys;

use crate::{console, events, script_limits, EvalError};

use super::Context;

impl<'rt> Context<'rt> {
    /// Evaluates `code` as global script and returns the result coerced to
    /// a string, mirroring `JS_ToCString`. On a JS-level exception, the
    /// actual thrown value is stringified into the `EvalError` (an
    /// `Error`'s own message, a thrown string verbatim), reported to the
    /// page like a real uncaught error would be — appended to
    /// `console`'s message buffer at error level and dispatched as an
    /// `error` event on `window` with a `message` property (listener
    /// exceptions raised *by* that dispatch are swallowed; reporting one
    /// uncaught error must not manufacture another) — before returning
    /// `Err`.
    pub fn eval(&self, code: &str, filename: &str) -> Result<String, EvalError> {
        let code_c = CString::new(code).expect("script source must not contain NUL bytes");
        let filename_c = CString::new(filename).expect("filename must not contain NUL bytes");

        let result = unsafe {
            script_limits::set_deadline(
                self.ptr,
                self._time_budget
                    .map(|budget| std::time::Instant::now() + budget),
            );
            let r = sys::JS_Eval(
                self.ptr,
                code_c.as_ptr(),
                code_c.as_bytes().len(),
                filename_c.as_ptr(),
                sys::JS_EVAL_TYPE_GLOBAL,
            );
            // The deadline only governs this one eval call - leaving it
            // stamped would let later unrelated work (timers, promise
            // jobs, another host's eval) hit a stale cutoff.
            script_limits::set_deadline(self.ptr, None);
            r
        };

        if sys::js_is_exception(&result) {
            unsafe { sys::JS_FreeValue(self.ptr, result) };
            let text = self.take_exception_text();
            self.report_uncaught_error(&text);
            return Err(EvalError(text));
        }

        let mut len: usize = 0;
        let c_str_ptr = unsafe { sys::JS_ToCStringLen2(self.ptr, &mut len, result, false) };
        unsafe { sys::JS_FreeValue(self.ptr, result) };

        if c_str_ptr.is_null() {
            return Err(EvalError("failed to stringify result".to_string()));
        }

        let owned = unsafe { CStr::from_ptr(c_str_ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe { sys::JS_FreeCString(self.ptr, c_str_ptr) };

        Ok(owned)
    }

    /// Pulls the pending exception off the context and stringifies it:
    /// `Error` instances render through their own `toString`
    /// ("SyntaxError: unexpected token"), thrown strings/primitives come
    /// out verbatim. A value that can't be stringified at all falls back
    /// to the old placeholder text.
    fn take_exception_text(&self) -> String {
        unsafe {
            let exception = sys::JS_GetException(self.ptr);
            // No pending exception (or a literal `null` was thrown - both
            // arrive as JS_NULL here): nothing better to say.
            if exception.tag == sys::JS_TAG_NULL {
                return "script raised an exception".to_string();
            }
            let mut len: usize = 0;
            let ptr = sys::JS_ToCStringLen2(self.ptr, &mut len, exception, false);
            sys::JS_FreeValue(self.ptr, exception);
            if ptr.is_null() {
                if sys::JS_HasException(self.ptr) {
                    let thrown = sys::JS_GetException(self.ptr);
                    sys::JS_FreeValue(self.ptr, thrown);
                }
                return "script raised an exception".to_string();
            }
            let text = CStr::from_ptr(ptr).to_string_lossy().into_owned();
            sys::JS_FreeCString(self.ptr, ptr);
            text
        }
    }

    /// The structured-error-reporting half of an uncaught script failure:
    /// the stringified exception lands in `console`'s buffer at error
    /// level (what a devtools console prints for an uncaught error) and a
    /// real non-bubbling `error` event carrying the same text in
    /// `.message` is dispatched on `window`/the global object, so pages
    /// can observe their own failures (`window.addEventListener("error",
    /// ...)`). Scope cut vs the spec: no `filename`/`lineno`/`colno`/
    /// `error` properties on the event, and the `window.onerror(message,
    /// source, ...)` function-property form isn't supported - only
    /// listener registration.
    fn report_uncaught_error(&self, text: &str) {
        unsafe {
            console::push_message(
                self.ptr,
                console::ConsoleMessage {
                    level: console::ConsoleLevel::Error,
                    text: format!("Uncaught {text}"),
                },
            );
            let global = sys::JS_GetGlobalObject(self.ptr);
            let event = events::create_event(self.ptr, "error", false, false);
            if !sys::js_is_exception(&event) {
                let name = CString::new("message").unwrap();
                let message = sys::JS_NewStringLen(
                    self.ptr,
                    text.as_ptr() as *const std::os::raw::c_char,
                    text.len(),
                );
                sys::JS_SetPropertyStr(self.ptr, event, name.as_ptr(), message);
                events::dispatch_event_object(self.ptr, global, event);
                // A listener throwing must not leave its exception pending
                // behind our back - this call already IS the error path.
                if sys::JS_HasException(self.ptr) {
                    let thrown = sys::JS_GetException(self.ptr);
                    sys::JS_FreeValue(self.ptr, thrown);
                }
            } else {
                let thrown = sys::JS_GetException(self.ptr);
                sys::JS_FreeValue(self.ptr, thrown);
            }
            sys::JS_FreeValue(self.ptr, global);
        }
    }
}
