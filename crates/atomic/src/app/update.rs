//! `eframe::App::update`, the per-frame entry point: pumps the automation
//! engine, then draws each panel in order.

use std::time::Duration;

use super::AtomicApp;

impl eframe::App for AtomicApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Every pane's process keeps rendering on its own real vsync loop
        // regardless of whether this GUI repaints - poll on a matching
        // cadence rather than only reacting to user input, or newly
        // published frames would sit unseen between interactions.
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(16));
        self.tick_automation_engine();

        self.draw_toolbar(ui);
        self.draw_automation_panel(ui);
        self.draw_add_profile_modal(&ctx);
        self.draw_settings_window(&ctx);
        self.draw_downloads_history_panel(ui);
        self.draw_pane_grid(ui);
    }
}
