//! The app-lifetime automation engine: running a script, rebinding it to
//! the current live panes, and ticking it every frame.

use atomic::automation_bridge;

use super::AtomicApp;

impl AtomicApp {
    /// Runs `self.automation_script`'s top-level code once against
    /// `self.automation_engine` - the app-lifetime engine every `update()`
    /// tick pumps (see `tick_automation_engine`) - scoped to every live
    /// pane (not just the selected one; a script names whichever pane it
    /// wants via `pane("pane-N")`). Unlike `run_automation_script_for_pane`
    /// (ephemeral, one pane, dropped when it returns), a script run here
    /// registers real `every`/`on`/cron callbacks that keep firing on
    /// every subsequent frame, because this engine is never dropped - see
    /// `automation_bridge::run_script`'s own doc for why the two entry
    /// points genuinely need to stay different tools.
    pub(super) fn run_automation_script(&mut self) {
        self.rebind_automation_engine();
        self.automation_result = Some(
            self.automation_engine
                .run(&self.automation_script, "shell-script.js")
                .map_err(|e| e.to_string()),
        );
    }

    /// Refreshes which real profiles `pane(...)` resolves to inside
    /// `self.automation_engine`, from whatever the active workspace's live
    /// panes are *right now* - called before every script run and every
    /// frame's `tick_automation_engine` so a pane added/closed/moved since
    /// the engine last ran is reflected without losing any JS state
    /// (registered callbacks, globals a script set) a fresh `AutomationEngine`
    /// would have lost.
    pub(super) fn rebind_automation_engine(&mut self) {
        let panes = automation_bridge::scoped_panes(
            &self.workspace,
            self.panes.iter().map(|p| (p.id.as_str(), &p.browser)),
        );
        self.automation_engine.rebind(panes);
    }

    /// Called once per `update()` frame (see `eframe::App::update`) - pumps
    /// `self.automation_engine`'s due timers/`requestAnimationFrame`/cron
    /// triggers (`AutomationEngine::tick`'s own doc), the real primitive
    /// that makes a script's `every`/`on`/`cron` registrations actually
    /// keep firing instead of only ever running once at eval time - closing
    /// the last half of the spec's automation-scripting gap (see
    /// CLAUDE.md). Rebinds first so a pane that just appeared/disappeared
    /// this frame is reflected before ticking against it.
    pub(super) fn tick_automation_engine(&mut self) {
        self.rebind_automation_engine();
        self.automation_engine.tick();
    }

    /// The context menu's "Run auto login" stand-in: runs
    /// `self.automation_script` scoped to *only* `self.panes[index]` (via a
    /// one-element pane list, so `pane("pane-N")` for any other id throws
    /// "no pane named" instead of silently reaching a different pane).
    /// The mockup's actual "auto login" implies a script saved *per
    /// profile* and run automatically - there's no such storage yet (see
    /// the spec's still-open "Settings: Automation ... editor+run log"
    /// gap), so this reuses the one shared script box scoped to one pane
    /// rather than pretending a per-profile script exists.
    pub(super) fn run_automation_script_for_pane(&mut self, index: usize) {
        let pane = &self.panes[index];
        let panes = std::iter::once((pane.id.as_str(), &pane.browser));
        self.automation_result = Some(automation_bridge::run_script(
            &self.workspace,
            panes,
            &self.automation_script,
        ));
    }
}
