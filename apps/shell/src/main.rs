use std::time::Duration;

use shell::browser_view::BrowserView;

const FRAME_WIDTH: u32 = 1024;
const FRAME_HEIGHT: u32 = 640;

struct NimbleApp {
    browser: BrowserView,
    address_bar_text: String,
}

impl Default for NimbleApp {
    fn default() -> Self {
        NimbleApp { browser: BrowserView::spawn(FRAME_WIDTH, FRAME_HEIGHT), address_bar_text: String::new() }
    }
}

impl NimbleApp {
    fn navigate_to_address_bar(&mut self) {
        let url = self.address_bar_text.trim().to_string();
        if !url.is_empty() {
            self.browser.navigate(&url);
        }
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
                if ui.button("Reload").clicked() {
                    self.browser.reload();
                }

                let address_bar = ui.add_sized(
                    [ui.available_width() - 8.0, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut self.address_bar_text).hint_text("Enter a URL and press Enter"),
                );
                if address_bar.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.navigate_to_address_bar();
                }
            });
            if let Some(error) = self.browser.error() {
                ui.colored_label(egui::Color32::RED, error);
            } else if let Some(error) = self.browser.navigation_error() {
                ui.colored_label(egui::Color32::RED, format!("Failed to load {}: {error}", self.browser.current_url()));
            }
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
