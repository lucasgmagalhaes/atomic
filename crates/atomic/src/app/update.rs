//! `eframe::App::update`, the per-frame entry point: pumps the automation
//! engine, syncs the chrome UI (push real state, drain real actions), then
//! draws the chrome browser's texture as the window background with pane
//! textures composited on top — see `spec/architecture/chrome-ui.md`.
//! Superseded by this: `draw_toolbar`/`draw_automation_panel`/
//! `draw_add_profile_modal`/`draw_settings_window`/
//! `draw_downloads_history_panel`, the old `egui`-drawn chrome panels
//! (their real `AtomicApp` methods are still used, now via
//! `chrome_ui_bridge`'s action dispatch instead of an `egui` button
//! click).

use std::time::Duration;

use super::AtomicApp;

/// The chrome page's grid screen only supports the mockup's four discrete
/// tiling modes (1/2/4/6, see `chrome-ui/index.html`'s `GRID_COLS`) - a
/// real per-pane-count layout isn't wired yet (same gap
/// `chrome_ui::ChromeAction`'s own doc flags for `setGrid`). Rounds the
/// *real* live pane count up to the nearest supported mode so the chrome
/// page renders at least that many `.pane` placeholders for
/// `ChromeUi::pane_rects` to find - an honest approximation, not real
/// per-count tiling.
fn nearest_supported_grid_size(pane_count: usize) -> u32 {
    match pane_count {
        0 | 1 => 1,
        2 => 2,
        3 | 4 => 4,
        _ => 6,
    }
}

impl eframe::App for AtomicApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Every pane's process keeps rendering on its own real vsync loop
        // regardless of whether this GUI repaints - poll on a matching
        // cadence rather than only reacting to user input, or newly
        // published frames would sit unseen between interactions.
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(16));
        self.tick_automation_engine();

        let grid = nearest_supported_grid_size(self.active_workspace_pane_indices().len());
        self.chrome.push_state(&format!(r#"{{"grid":{grid}}}"#));

        for action in self.chrome.drain_actions() {
            self.handle_chrome_action(action);
        }

        self.draw_pane_grid(ui);
    }
}
