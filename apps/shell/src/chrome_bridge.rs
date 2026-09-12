//! Native bridge exposed to the chrome engine's own JS as
//! `globalThis.atomic.{addProfile, setPaneCount}` — the spike's proof that
//! chrome-as-HTML can drive real `AtomicApp` state, not just render static
//! markup. Mirrors `automation::cron`'s registration pattern (a
//! thread-local registry keyed by the owning `JSContext` pointer, since
//! this is a second, independent embedding of `js-runtime` with its own
//! native globals, not a class every embedding shares).

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

#[derive(Debug, Clone)]
pub(crate) enum ChromeAction {
    AddProfile,
    SetPaneCount(usize),
    SwitchWorkspace(usize),
    CreateWorkspace,
    SetLocale(String),
    /// A download URL submitted from the downloads/history chrome panel.
    /// Unlike the other variants, this is never pushed from a native JS
    /// function's `argv` — the submitted `<input>`'s live `.value` is a
    /// `dom::Dom` field independent of any HTML attribute (see
    /// `dom::Dom::value`'s own doc), so `ChromeEngine::handle_click` reads
    /// it directly off the DOM before dispatching the click, and calls
    /// [`push_download_submitted`] itself instead of routing through a
    /// registered global function.
    DownloadSubmitted(String),
    CreateProfileSubmit,
    CancelAddProfile,
    StepMaxPanes(i32),
    StepFpsCap(i32),
    SetFpsCapEnabled(bool),
    SetUseKeychain(bool),
    /// `None` is the "Default" adapter, mirroring
    /// `PerformanceSettings::gpu_adapter`'s own `Option<usize>` shape.
    ApplyGpuAdapter(Option<usize>),
    /// The two credential fields are read directly off the DOM at click
    /// time, same reasoning [`ChromeAction::DownloadSubmitted`] documents —
    /// simpler than passing them back through JS argv.
    AddCredential,
    RemoveCredential(usize),
    ImportHistory,
    ImportBookmarks,
    ImportCookies,
    ImportPasswords,
    CloseSettings,
}

thread_local! {
    static ACTIONS: RefCell<HashMap<usize, Vec<ChromeAction>>> = RefCell::new(HashMap::new());
}

fn push_action(ctx: *mut sys::JSContext, action: ChromeAction) {
    ACTIONS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .push(action);
    });
}

/// Same "stringify then parse" trick this codebase already uses elsewhere
/// (see `profile_worker::input_commands::read_js_string`-shaped helpers) —
/// no safe binding for `JS_ToFloat64` exists in `quickjs-sys` yet, and this
/// is a plain numeric argument, not a hot path.
unsafe fn read_js_number(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<f64> {
    read_js_string(ctx, val).and_then(|s| s.trim().parse::<f64>().ok())
}

/// Same "stringify via `JS_ToCStringLen2`" convention `automation::cron`'s
/// own `read_js_string` uses for reading an arbitrary JS argument back into
/// Rust.
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

unsafe extern "C" fn add_profile_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::AddProfile);
    sys::js_undefined()
}

unsafe extern "C" fn set_pane_count_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    if let Some(n) = read_js_number(ctx, *argv) {
        push_action(ctx, ChromeAction::SetPaneCount(n.max(0.0) as usize));
    }
    sys::js_undefined()
}

unsafe extern "C" fn switch_workspace_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    if let Some(n) = read_js_number(ctx, *argv) {
        push_action(ctx, ChromeAction::SwitchWorkspace(n.max(0.0) as usize));
    }
    sys::js_undefined()
}

unsafe extern "C" fn create_workspace_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::CreateWorkspace);
    sys::js_undefined()
}

unsafe extern "C" fn set_locale_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    if let Some(s) = read_js_string(ctx, *argv) {
        push_action(ctx, ChromeAction::SetLocale(s));
    }
    sys::js_undefined()
}

/// Registers one `atomic.<name>` native function on `atomic` — the shared
/// primitive [`register`] builds every entry from, added when the list grew
/// past the point where inlining each `CString`/`JS_NewCFunction2`/
/// `JS_SetPropertyStr` triplet by hand was worth it.
unsafe fn register_fn(
    ctx: *mut sys::JSContext,
    atomic: sys::JSValue,
    name: &str,
    func: sys::JSCFunction,
    arity: c_int,
) {
    let name_c = CString::new(name).expect("bridge function name must not contain NUL bytes");
    let f = sys::JS_NewCFunction2(ctx, func, name_c.as_ptr(), arity, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, atomic, name_c.as_ptr(), f);
}

/// Registers `globalThis.atomic = { addProfile(), setPaneCount(n), ... }` on
/// `ctx`. Call once per `Context`, same convention every other native
/// global in this workspace follows (`automation::cron::register`, etc.).
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    let atomic = sys::JS_NewObject(ctx);

    register_fn(ctx, atomic, "addProfile", add_profile_fn, 0);
    register_fn(ctx, atomic, "setPaneCount", set_pane_count_fn, 1);
    register_fn(ctx, atomic, "switchWorkspace", switch_workspace_fn, 1);
    register_fn(ctx, atomic, "createWorkspace", create_workspace_fn, 0);
    register_fn(ctx, atomic, "setLocale", set_locale_fn, 1);
    register_fn(
        ctx,
        atomic,
        "createProfileSubmit",
        create_profile_submit_fn,
        0,
    );
    register_fn(ctx, atomic, "cancelAddProfile", cancel_add_profile_fn, 0);
    register_fn(ctx, atomic, "stepMaxPanes", step_max_panes_fn, 1);
    register_fn(ctx, atomic, "stepFpsCap", step_fps_cap_fn, 1);
    register_fn(ctx, atomic, "setFpsCapEnabled", set_fps_cap_enabled_fn, 1);
    register_fn(ctx, atomic, "setUseKeychain", set_use_keychain_fn, 1);
    register_fn(ctx, atomic, "applyGpuAdapter", apply_gpu_adapter_fn, 1);
    register_fn(ctx, atomic, "addCredential", add_credential_fn, 0);
    register_fn(ctx, atomic, "removeCredential", remove_credential_fn, 1);
    register_fn(ctx, atomic, "importHistory", import_history_fn, 0);
    register_fn(ctx, atomic, "importBookmarks", import_bookmarks_fn, 0);
    register_fn(ctx, atomic, "importCookies", import_cookies_fn, 0);
    register_fn(ctx, atomic, "importPasswords", import_passwords_fn, 0);
    register_fn(ctx, atomic, "closeSettings", close_settings_fn, 0);

    let atomic_name = CString::new("atomic").unwrap();
    sys::JS_SetPropertyStr(ctx, global, atomic_name.as_ptr(), atomic);
    sys::JS_FreeValue(ctx, global);
}

unsafe extern "C" fn create_profile_submit_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::CreateProfileSubmit);
    sys::js_undefined()
}

unsafe extern "C" fn cancel_add_profile_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::CancelAddProfile);
    sys::js_undefined()
}

unsafe extern "C" fn step_max_panes_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    if let Some(n) = read_js_number(ctx, *argv) {
        push_action(ctx, ChromeAction::StepMaxPanes(n as i32));
    }
    sys::js_undefined()
}

unsafe extern "C" fn step_fps_cap_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    if let Some(n) = read_js_number(ctx, *argv) {
        push_action(ctx, ChromeAction::StepFpsCap(n as i32));
    }
    sys::js_undefined()
}

unsafe extern "C" fn set_fps_cap_enabled_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    if let Some(n) = read_js_number(ctx, *argv) {
        push_action(ctx, ChromeAction::SetFpsCapEnabled(n != 0.0));
    }
    sys::js_undefined()
}

unsafe extern "C" fn set_use_keychain_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    if let Some(n) = read_js_number(ctx, *argv) {
        push_action(ctx, ChromeAction::SetUseKeychain(n != 0.0));
    }
    sys::js_undefined()
}

unsafe extern "C" fn apply_gpu_adapter_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    if let Some(n) = read_js_number(ctx, *argv) {
        let adapter = if n < 0.0 { None } else { Some(n as usize) };
        push_action(ctx, ChromeAction::ApplyGpuAdapter(adapter));
    }
    sys::js_undefined()
}

unsafe extern "C" fn add_credential_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::AddCredential);
    sys::js_undefined()
}

unsafe extern "C" fn remove_credential_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    if let Some(n) = read_js_number(ctx, *argv) {
        push_action(ctx, ChromeAction::RemoveCredential(n.max(0.0) as usize));
    }
    sys::js_undefined()
}

unsafe extern "C" fn import_history_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::ImportHistory);
    sys::js_undefined()
}

unsafe extern "C" fn import_bookmarks_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::ImportBookmarks);
    sys::js_undefined()
}

unsafe extern "C" fn import_cookies_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::ImportCookies);
    sys::js_undefined()
}

unsafe extern "C" fn import_passwords_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::ImportPasswords);
    sys::js_undefined()
}

unsafe extern "C" fn close_settings_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::CloseSettings);
    sys::js_undefined()
}

/// Pushes a `DownloadSubmitted` action for `ctx` — called directly from
/// `ChromeEngine::handle_click` (not from a registered native function;
/// see [`ChromeAction::DownloadSubmitted`]'s own doc for why).
pub(crate) fn push_download_submitted(ctx: *mut sys::JSContext, url: String) {
    push_action(ctx, ChromeAction::DownloadSubmitted(url));
}

/// Drains and returns every action `ctx`'s JS has queued via `atomic.*`
/// since the last call — `AtomicApp::update` calls this once per frame and
/// dispatches each action into its own real methods (`panes::create_profile`,
/// `panes::set_pane_count`), so a chrome-engine button click ends up doing
/// exactly what the equivalent egui button does today.
pub(crate) fn drain_actions(ctx: *mut sys::JSContext) -> Vec<ChromeAction> {
    ACTIONS.with(|reg| reg.borrow_mut().remove(&(ctx as usize)).unwrap_or_default())
}
