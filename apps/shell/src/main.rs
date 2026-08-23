use std::time::Duration;

use shell::browser_view::BrowserView;

const FRAME_WIDTH: u32 = 1024;
const FRAME_HEIGHT: u32 = 640;

struct NimbleApp {
    browser: BrowserView,
}

impl Default for NimbleApp {
    fn default() -> Self {
        NimbleApp { browser: BrowserView::spawn(FRAME_WIDTH, FRAME_HEIGHT) }
    }
}

impl eframe::App for NimbleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // The profile process keeps rendering on its own real vsync loop
        // regardless of whether this GUI repaints - poll it on a matching
        // cadence rather than only reacting to user input, or newly
        // published frames would sit unseen between interactions.
        ctx.request_repaint_after(Duration::from_millis(16));

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Nimble");
                if ui.button("Reload").clicked() {
                    self.browser.reload();
                }
                if let Some(error) = self.browser.error() {
                    ui.colored_label(egui::Color32::RED, error);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(texture) = self.browser.poll_texture(ctx) {
                let available = ui.available_size();
                let image_size = texture.size_vec2();
                let scale = (available.x / image_size.x).min(available.y / image_size.y).min(1.0);
                ui.image((texture.id(), image_size * scale));
            } else if self.browser.error().is_none() {
                ui.label("Starting profile process...");
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native("Nimble", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(NimbleApp::default()))))
}
