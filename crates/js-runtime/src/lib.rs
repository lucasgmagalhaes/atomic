//! Safe wrapper around `quickjs-sys`. Minimal on purpose (phase 2 slice):
//! create a runtime/context and eval JS to a string. DOM↔JS bindings land
//! in a later pass once `dom` has something worth exposing.

use std::ffi::{CStr, CString};
use std::marker::PhantomData;

use quickjs_sys as sys;

mod crypto;
mod dom_bindings;
mod performance;

#[derive(Debug)]
pub struct EvalError(pub String);

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JS exception: {}", self.0)
    }
}

impl std::error::Error for EvalError {}

pub struct Runtime {
    ptr: *mut sys::JSRuntime,
}

impl Runtime {
    pub fn new() -> Self {
        let ptr = unsafe { sys::JS_NewRuntime() };
        assert!(!ptr.is_null(), "JS_NewRuntime returned null");
        Runtime { ptr }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe { sys::JS_FreeRuntime(self.ptr) };
    }
}

// The runtime owns no thread-local state we rely on here, but QuickJS
// runtimes are not meant to be touched from multiple threads at once.
// Leave Runtime !Send/!Sync (the default for a raw-pointer field) until
// the profile/ipc process-per-tab model defines how this actually gets
// used across threads.

pub struct Context<'rt> {
    ptr: *mut sys::JSContext,
    _runtime: PhantomData<&'rt Runtime>,
    // Kept alive here (not just handed to JS_SetContextOpaque) so it's freed
    // on Context::drop instead of leaking. Moving the Box moves this struct's
    // pointer field, not the heap allocation, so the raw pointer registered
    // with QuickJS via register() stays valid regardless.
    _dom: Option<Box<dom::Dom>>,
}

impl<'rt> Context<'rt> {
    pub fn new(runtime: &'rt Runtime) -> Self {
        let ptr = unsafe { sys::JS_NewContext(runtime.ptr) };
        assert!(!ptr.is_null(), "JS_NewContext returned null");
        unsafe {
            performance::register(ptr);
            crypto::register(ptr);
        };
        Context {
            ptr,
            _runtime: PhantomData,
            _dom: None,
        }
    }

    /// Same as [`Context::new`], but also registers the minimal DOM
    /// bindings (see `dom_bindings`) backed by `dom`.
    pub fn with_dom(runtime: &'rt Runtime, dom: dom::Dom) -> Self {
        let mut ctx = Self::new(runtime);
        let mut dom_box = Box::new(dom);
        let raw = dom_box.as_mut() as *mut dom::Dom as *mut std::os::raw::c_void;
        unsafe {
            sys::JS_SetContextOpaque(ctx.ptr, raw);
            dom_bindings::register(ctx.ptr);
        }
        ctx._dom = Some(dom_box);
        ctx
    }

    /// Evaluates `code` as global script and returns the result coerced to
    /// a string, mirroring `JS_ToCString`. On a JS-level exception, returns
    /// `Err` with a placeholder message — hooking up `JS_GetException` for
    /// a real error value/stack is follow-up work, not yet bound.
    pub fn eval(&self, code: &str, filename: &str) -> Result<String, EvalError> {
        let code_c = CString::new(code).expect("script source must not contain NUL bytes");
        let filename_c =
            CString::new(filename).expect("filename must not contain NUL bytes");

        let result = unsafe {
            sys::JS_Eval(
                self.ptr,
                code_c.as_ptr(),
                code_c.as_bytes().len(),
                filename_c.as_ptr(),
                sys::JS_EVAL_TYPE_GLOBAL,
            )
        };

        if sys::js_is_exception(&result) {
            unsafe { sys::JS_FreeValue(self.ptr, result) };
            return Err(EvalError("script raised an exception".to_string()));
        }

        let mut len: usize = 0;
        let c_str_ptr =
            unsafe { sys::JS_ToCStringLen2(self.ptr, &mut len, result, false) };
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
}

impl Drop for Context<'_> {
    fn drop(&mut self) {
        unsafe { sys::JS_FreeContext(self.ptr) };
    }
}
