//! Builds an `automation::AutomationEngine` from a `workspace::WorkspaceManager`
//! and however many live panes a caller hands it, and runs one script
//! against it. Pulled out of `main.rs`'s `NimbleApp` deliberately: this
//! logic touches no `egui` type, so it's testable head-on (spawn real
//! `BrowserView`s, run a real script, assert on the result) without
//! driving the actual GUI window — `NimbleApp::run_automation_script` is a
//! thin wrapper around [`run_script`] plus storing the result for display.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::browser_view::BrowserView;
use crate::workspace::WorkspaceManager;

/// Builds the pane map a script is allowed to see: whichever `panes` (id,
/// `BrowserView`) pairs have both a live `Profile` *and* an id listed in
/// `workspace`'s *active* workspace right now. A pane spawned but not
/// (yet) registered into the active workspace, or an id in the workspace
/// with no corresponding live pane, is correctly left out — see
/// `automation::AutomationEngine::new`'s own doc on why that's deliberate,
/// not an oversight: a script's blast radius should be exactly what the
/// host granted it. Takes `&BrowserView` (not `&mut`) since
/// `profile_handle` only clones an `Rc` — building this map never needs
/// exclusive access to the panes themselves.
pub fn scoped_panes<'p>(
    workspace: &WorkspaceManager,
    panes: impl IntoIterator<Item = (&'p str, &'p BrowserView)>,
) -> HashMap<String, Rc<RefCell<profile::Profile>>> {
    let active_ids = workspace.active().profiles();
    let mut engine_panes = HashMap::new();
    for (id, browser) in panes {
        if !active_ids.iter().any(|active_id| active_id == id) {
            continue;
        }
        if let Some(profile) = browser.profile_handle() {
            engine_panes.insert(id.to_string(), profile);
        }
    }
    engine_panes
}

/// Runs `script`'s top-level code once against a fresh, ephemeral engine
/// scoped to `panes` (see [`scoped_panes`]) - registers `every`/`on`/cron
/// callbacks but the engine is dropped when this returns, so none of them
/// ever fire. For a script whose scheduled callbacks need to keep firing
/// across GUI frames, see `main.rs`'s persistent
/// `NimbleApp::automation_engine` + `tick_automation_engine` instead - this
/// remains the right tool for a script that's genuinely one-shot (the
/// context menu's "Run auto login," automation_bridge's own tests).
///
/// `Ok` is the eval's stringified result; `Err` is the JS exception
/// message (covers a genuinely bad script, an unknown pane name, and
/// `pane.fill`/`pane.click` failing against a real element that doesn't
/// exist — see the `automation` crate's own doc for what's real there now).
pub fn run_script<'p>(
    workspace: &WorkspaceManager,
    panes: impl IntoIterator<Item = (&'p str, &'p BrowserView)>,
    script: &str,
) -> Result<String, String> {
    let runtime = js_runtime::Runtime::new();
    let engine = automation::AutomationEngine::new(&runtime, scoped_panes(workspace, panes));
    engine
        .run(script, "shell-script.js")
        .map_err(|e| e.to_string())
}
