//! The per-context `ACTIONS` queue, `push_action`, `push_download_submitted`,
//! and `drain_actions` — split out from `chrome_bridge.rs`.

use std::cell::RefCell;
use std::collections::HashMap;

use quickjs_sys as sys;

use super::action::ChromeAction;

thread_local! {
    static ACTIONS: RefCell<HashMap<usize, Vec<ChromeAction>>> = RefCell::new(HashMap::new());
}

pub(super) fn push_action(ctx: *mut sys::JSContext, action: ChromeAction) {
    ACTIONS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .push(action);
    });
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
