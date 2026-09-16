//! Every registered `atomic.<name>` native function — split out from
//! `chrome_bridge.rs`.

use std::os::raw::c_int;

use neutron::quickjs_sys as sys;

use super::action::ChromeAction;
use super::helpers::{read_js_number, read_js_string};
use super::registry::push_action;

pub(super) unsafe extern "C" fn add_profile_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::AddProfile);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn set_pane_count_fn(
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

pub(super) unsafe extern "C" fn switch_workspace_fn(
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

pub(super) unsafe extern "C" fn create_workspace_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::CreateWorkspace);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn set_locale_fn(
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

pub(super) unsafe extern "C" fn create_profile_submit_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::CreateProfileSubmit);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn cancel_add_profile_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::CancelAddProfile);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn step_max_panes_fn(
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

pub(super) unsafe extern "C" fn step_fps_cap_fn(
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

pub(super) unsafe extern "C" fn set_fps_cap_enabled_fn(
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

pub(super) unsafe extern "C" fn set_use_keychain_fn(
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

pub(super) unsafe extern "C" fn apply_gpu_adapter_fn(
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

pub(super) unsafe extern "C" fn add_credential_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::AddCredential);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn remove_credential_fn(
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

pub(super) unsafe extern "C" fn import_history_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::ImportHistory);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn import_bookmarks_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::ImportBookmarks);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn import_cookies_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::ImportCookies);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn import_passwords_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::ImportPasswords);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn close_settings_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    push_action(ctx, ChromeAction::CloseSettings);
    sys::js_undefined()
}
