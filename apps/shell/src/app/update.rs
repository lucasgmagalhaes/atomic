//! `eframe::App::update`, the per-frame entry point: pumps the automation
//! engine, then draws each panel in order.

use std::time::Duration;

use super::NimbleApp;

impl eframe::App for NimbleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Every pane's process keeps rendering on its own real vsync loop
        // regardless of whether this GUI repaints - poll on a matching
        // cadence rather than only reacting to user input, or newly
        // published frames would sit unseen between interactions.
        ctx.request_repaint_after(Duration::from_millis(16));
        self.tick_automation_engine();

        self.draw_toolbar(ctx);
        self.draw_chrome_toolbar_spike(ctx);
        self.draw_automation_panel(ctx);
        self.draw_add_profile_modal(ctx);
        self.draw_chrome_add_profile_spike(ctx);
        self.draw_settings_window(ctx);
        self.draw_chrome_settings_spike(ctx);
        self.draw_downloads_history_panel(ctx);
        self.draw_chrome_downloads_history_spike(ctx);
        self.draw_pane_grid(ctx);
    }
}
