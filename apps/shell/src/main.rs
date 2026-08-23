use std::time::Duration;

use shell::browser_view::BrowserView;

const FRAME_WIDTH: u32 = 1024;
const FRAME_HEIGHT: u32 = 640;

struct NimbleApp {
    browser: BrowserView,
    address_bar_text: String,
    /// The proxy text box's current contents - not necessarily what the
    /// live profile is actually using, since a proxy only takes effect on
    /// [`apply_proxy`](Self::apply_proxy) (it can't be changed on an
    /// already-running profile - see `BrowserView::spawn_with_proxy`'s
    /// doc). Starts empty (no proxy), matching `BrowserView::spawn`'s own
    /// default.
    proxy_text: String,
    /// The proxy the currently running profile was actually spawned with
    /// (`None` for no proxy) - shown next to the text box so it's obvious
    /// when an edit hasn't been applied yet.
    active_proxy: Option<String>,
}

impl Default for NimbleApp {
    fn default() -> Self {
        NimbleApp {
            browser: BrowserView::spawn(FRAME_WIDTH, FRAME_HEIGHT),
            address_bar_text: String::new(),
            proxy_text: String::new(),
            active_proxy: None,
        }
    }
}

impl NimbleApp {
    fn navigate_to_address_bar(&mut self) {
        let url = self.address_bar_text.trim().to_string();
        if !url.is_empty() {
            self.browser.navigate(&url);
        }
    }

    /// Respawns the profile with the proxy text box's current contents -
    /// an empty box means "no proxy". A running profile's own proxy is
    /// fixed for its process lifetime (matches `profile-worker`'s "set
    /// once at spawn" scope), so applying a change here is a real
    /// respawn, not a live setting - the address bar and current page are
    /// lost, same as changing a profile's proxy in a real browser would
    /// require a restart of that profile's process.
    fn apply_proxy(&mut self) {
        let proxy = self.proxy_text.trim();
        let proxy = if proxy.is_empty() { None } else { Some(proxy) };
        self.browser = BrowserView::spawn_with_proxy(FRAME_WIDTH, FRAME_HEIGHT, proxy);
        self.active_proxy = proxy.map(str::to_string);
        self.address_bar_text.clear();
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
            ui.horizontal(|ui| {
                ui.label("Proxy:");
                let proxy_field = ui.add_sized(
                    [220.0, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut self.proxy_text).hint_text("host:port (empty = none)"),
                );
                let apply_clicked = ui.button("Apply").clicked();
                let applied_via_enter = proxy_field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if apply_clicked || applied_via_enter {
                    self.apply_proxy();
                }
                match &self.active_proxy {
                    Some(proxy) => ui.label(format!("active: {proxy}")),
                    None => ui.label("active: none"),
                };
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
