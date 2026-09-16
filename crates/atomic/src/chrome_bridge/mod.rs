//! Native bridge exposed to the chrome engine's own JS as
//! `globalThis.atomic.{addProfile, setPaneCount}` — the spike's proof that
//! chrome-as-HTML can drive real `AtomicApp` state, not just render static
//! markup. Mirrors `automation::cron`'s registration pattern (a
//! thread-local registry keyed by the owning `JSContext` pointer, since
//! this is a second, independent embedding of `js-runtime` with its own
//! native globals, not a class every embedding shares).
//!
//! Split into `action.rs` (`ChromeAction`), `registry.rs` (the per-context
//! action queue, `push_download_submitted`, `drain_actions`),
//! `helpers.rs` (`read_js_number`/`read_js_string`/`register_fn`), and
//! `functions.rs` (every registered `atomic.<name>` native function) —
//! this file keeps the public `register` entry point.

use std::ffi::CString;

use neutron::quickjs_sys as sys;

mod action;
mod functions;
mod helpers;
mod registry;

pub(crate) use action::ChromeAction;
pub(crate) use registry::drain_actions;
#[allow(unused_imports)]
pub(crate) use registry::push_download_submitted;

use functions::{
    add_credential_fn, add_profile_fn, apply_gpu_adapter_fn, cancel_add_profile_fn,
    close_settings_fn, create_profile_submit_fn, create_workspace_fn, import_bookmarks_fn,
    import_cookies_fn, import_history_fn, import_passwords_fn, remove_credential_fn,
    set_fps_cap_enabled_fn, set_locale_fn, set_pane_count_fn, set_use_keychain_fn, step_fps_cap_fn,
    step_max_panes_fn, switch_workspace_fn,
};
use helpers::register_fn;

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
