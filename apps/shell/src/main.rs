use std::time::Duration;

use shell::browser_view::BrowserView;
use shell::workspace::WorkspaceManager;

const FRAME_WIDTH: u32 = 1024;
const FRAME_HEIGHT: u32 = 640;

/// The pane name the currently running `BrowserView`'s profile is
/// registered under in the active workspace, and therefore the only name
/// an automation script can pass to `pane(...)` right now — `apps/shell`
/// only ever spawns one live profile process at a time (see
/// `BrowserView`'s own doc comment), so this is the one entry
/// `WorkspaceManager::active().profiles()` can actually resolve to a
/// running `Profile`. A future multi-pane grid would give each spawned
/// profile its own id here instead of one constant.
const MAIN_PANE_ID: &str = "main";

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
    /// Named groups of profiles (mockup's workspace switcher). Real data
    /// layer, still no switcher UI here (see `workspace::WorkspaceManager`'s
    /// own doc) - its only role in this GUI so far is naming the pane an
    /// automation script can address, so `pane("main")` resolves against a
    /// real id instead of a hardcoded string only `automation` knows about.
    workspace: WorkspaceManager,
    automation_script: String,
    /// `Ok(result)` from the last `pane(...).goto(...)`-style script's
    /// top-level eval, or `Err(message)` if it raised - e.g. `pane.fill`/
    /// `pane.click` always land here (see `automation`'s own doc on why
    /// those still throw). `None` before any script has run.
    automation_result: Option<Result<String, String>>,
}

impl Default for NimbleApp {
    fn default() -> Self {
        let mut workspace = WorkspaceManager::new();
        let active = workspace.active_index();
        workspace.add_profile(active, MAIN_PANE_ID.to_string());

        NimbleApp {
            browser: BrowserView::spawn(FRAME_WIDTH, FRAME_HEIGHT),
            address_bar_text: String::new(),
            proxy_text: String::new(),
            active_proxy: None,
            workspace,
            automation_script: String::new(),
            automation_result: None,
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

    /// Runs `self.automation_script`'s top-level code once against whatever
    /// panes from the *active workspace* actually have a live `Profile`
    /// right now - built fresh from `self.workspace` each call rather than
    /// kept around, so pane names always reflect the current workspace
    /// instead of a stale snapshot.
    ///
    /// Ephemeral by design, not a limitation to fix later in this pass: the
    /// `AutomationEngine`/`js_runtime::Runtime` it builds are dropped at the
    /// end of this call, same as this whole method's stack frame. A script
    /// that calls `every`/`on`/`cron` registers those callbacks into a
    /// `Context` nothing will ever `tick()` again after this returns - they
    /// silently never fire. A real "keep the engine alive across frames and
    /// call `tick()` from `update`" wiring needs `NimbleApp` to hold an
    /// `AutomationEngine` borrowing `self.browser`'s `Profile` at the same
    /// time as `self.browser` itself is used elsewhere in `update` - a
    /// self-referential-struct problem this pass doesn't take on. Good
    /// enough for the mockup's "editor + run" half of automation; the
    /// "watchdog reconexão" / persistent-schedule half stays a gap.
    fn run_automation_script(&mut self) {
        let runtime = js_runtime::Runtime::new();
        let mut panes = std::collections::HashMap::new();
        // A loop over `self.workspace.active().profiles()` calling
        // `self.browser.profile_mut()` per iteration doesn't borrow-check
        // (NLL can't see the active workspace has at most one id this app
        // can resolve to a live profile) - written as a single lookup
        // instead. Any *other* id in the active workspace has no spawned
        // process behind it (`apps/shell` only ever runs one `BrowserView`
        // - see `MAIN_PANE_ID`'s doc), so it's correctly left out of
        // `panes`: `pane("that-id")` throws automation's real "no pane
        // named" error rather than something misleading.
        if self.workspace.active().profiles().iter().any(|id| id == MAIN_PANE_ID) {
            if let Some(profile) = self.browser.profile_mut() {
                panes.insert(MAIN_PANE_ID.to_string(), profile);
            }
        }

        let engine = automation::AutomationEngine::new(&runtime, panes);
        self.automation_result = Some(engine.run(&self.automation_script, "shell-script.js").map_err(|e| e.to_string()));
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

        egui::TopBottomPanel::bottom("automation").show(ctx, |ui| {
            ui.label(format!("Automation — pane names from the active workspace (\"{}\")", self.workspace.active().name));
            ui.add(
                egui::TextEdit::multiline(&mut self.automation_script)
                    .hint_text(format!("pane(\"{MAIN_PANE_ID}\").goto(\"https://example.com\")"))
                    .desired_rows(3),
            );
            if ui.button("Run").clicked() {
                self.run_automation_script();
            }
            match &self.automation_result {
                Some(Ok(result)) => {
                    ui.colored_label(egui::Color32::GREEN, format!("-> {result}"));
                }
                Some(Err(error)) => {
                    ui.colored_label(egui::Color32::RED, error);
                }
                None => {}
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
