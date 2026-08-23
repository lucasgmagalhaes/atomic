//! Builds an `automation::AutomationEngine` from a `workspace::WorkspaceManager`
//! and a `BrowserView`, and runs one script against it. Pulled out of
//! `main.rs`'s `NimbleApp` deliberately: this logic touches no `egui`
//! type, so it's testable head-on (spawn a real `BrowserView`, run a real
//! script, assert on the result) without driving the actual GUI window —
//! `NimbleApp::run_automation_script` is a two-line wrapper around
//! [`run_script`] plus storing the result for display.
use std::collections::HashMap;

use crate::browser_view::BrowserView;
use crate::workspace::WorkspaceManager;

/// The pane name the currently running `BrowserView`'s profile is
/// registered under in the active workspace, and therefore the only name
/// an automation script can pass to `pane(...)` right now — `apps/shell`
/// only ever spawns one live profile process at a time (see
/// `BrowserView`'s own doc comment), so this is the one entry
/// `WorkspaceManager::active().profiles()` can actually resolve to a
/// running `Profile`. A future multi-pane grid would give each spawned
/// profile its own id here instead of one constant.
pub const MAIN_PANE_ID: &str = "main";

/// Runs `script`'s top-level code once against whatever panes from
/// `workspace`'s *active* workspace actually have a live `Profile` behind
/// them right now (currently at most one: `MAIN_PANE_ID`, if present and
/// `browser` spawned successfully — see [`MAIN_PANE_ID`]'s doc). `Ok` is
/// the eval's stringified result; `Err` is the JS exception message,
/// covering both a genuinely bad script and `pane.fill`/`pane.click`'s
/// deliberate "not implemented" throw (see the `automation` crate's own
/// doc).
///
/// Ephemeral by design: the `AutomationEngine`/`js_runtime::Runtime` built
/// here are dropped when this function returns, so `every`/`on`/`cron`
/// callbacks a script registers never fire after that — see
/// `NimbleApp::run_automation_script`'s doc for why keeping one alive
/// across GUI frames is a separate, harder problem this doesn't take on.
pub fn run_script(workspace: &WorkspaceManager, browser: &mut BrowserView, script: &str) -> Result<String, String> {
    let runtime = js_runtime::Runtime::new();
    let mut panes: HashMap<String, &mut profile::Profile> = HashMap::new();
    if workspace.active().profiles().iter().any(|id| id == MAIN_PANE_ID) {
        if let Some(profile) = browser.profile_mut() {
            panes.insert(MAIN_PANE_ID.to_string(), profile);
        }
    }

    let engine = automation::AutomationEngine::new(&runtime, panes);
    engine.run(script, "shell-script.js").map_err(|e| e.to_string())
}
