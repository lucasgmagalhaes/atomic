//! Builds an `automation::AutomationEngine` from a `workspace::WorkspaceManager`
//! and however many live panes a caller hands it, and runs one script
//! against it. Pulled out of `main.rs`'s `NimbleApp` deliberately: this
//! logic touches no `egui` type, so it's testable head-on (spawn real
//! `BrowserView`s, run a real script, assert on the result) without
//! driving the actual GUI window — `NimbleApp::run_automation_script` is a
//! thin wrapper around [`run_script`] plus storing the result for display.
use std::collections::HashMap;

use crate::browser_view::BrowserView;
use crate::workspace::WorkspaceManager;

/// Runs `script`'s top-level code once against whatever `panes` (id,
/// `BrowserView`) pairs actually have both a live `Profile` *and* an id
/// listed in `workspace`'s *active* workspace right now. A pane spawned but
/// not (yet) registered into the active workspace, or an id in the
/// workspace with no corresponding live pane, is correctly left out of the
/// script's `pane(...)` scope — see `automation::AutomationEngine::new`'s
/// own doc on why that's deliberate, not an oversight: a script's blast
/// radius should be exactly what the host granted it.
///
/// `Ok` is the eval's stringified result; `Err` is the JS exception
/// message (covers a genuinely bad script, an unknown pane name, and
/// `pane.fill`/`pane.click` failing against a real element that doesn't
/// exist — see the `automation` crate's own doc for what's real there now).
///
/// Ephemeral by design: the `AutomationEngine`/`js_runtime::Runtime` built
/// here are dropped when this function returns, so `every`/`on`/`cron`
/// callbacks a script registers never fire after that — see
/// `NimbleApp::run_automation_script`'s doc for why keeping one alive
/// across GUI frames is a separate, harder problem this doesn't take on.
pub fn run_script<'p>(workspace: &WorkspaceManager, panes: impl IntoIterator<Item = (&'p str, &'p mut BrowserView)>, script: &str) -> Result<String, String> {
    let runtime = js_runtime::Runtime::new();
    let active_ids = workspace.active().profiles();
    let mut engine_panes: HashMap<String, &mut profile::Profile> = HashMap::new();
    for (id, browser) in panes {
        if !active_ids.iter().any(|active_id| active_id == id) {
            continue;
        }
        if let Some(profile) = browser.profile_mut() {
            engine_panes.insert(id.to_string(), profile);
        }
    }

    let engine = automation::AutomationEngine::new(&runtime, engine_panes);
    engine.run(script, "shell-script.js").map_err(|e| e.to_string())
}
