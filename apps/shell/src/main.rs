use std::time::Duration;

use shell::automation_bridge;
use shell::browser_view::BrowserView;
use shell::downloads::Downloads;
use shell::history::History;
use shell::i18n::{self, Locale};
use shell::resource_monitor::PaneMonitor;
use shell::settings::PerformanceSettings;
use shell::tiling;
use shell::vault_ui;
use shell::workspace::WorkspaceManager;

const PANE_WIDTH: u32 = 512;
const PANE_HEIGHT: u32 = 320;

/// One live grid cell: a stable id (registered into the active workspace
/// so `automation_bridge::run_script` can address it via `pane("pane-N")`)
/// plus the `BrowserView` actually rendering it. Ids are assigned
/// positionally (`pane-1`, `pane-2`, ...) and reassigned whenever the pane
/// count changes - stable identity across a session isn't needed yet since
/// there's no per-pane persistence (proxy, last URL) surviving a count
/// change either; see `NimbleApp::set_pane_count`'s doc.
struct Pane {
    id: String,
    browser: BrowserView,
    /// Real CPU/RAM/FPS telemetry for this pane's process - see
    /// `resource_monitor`'s own doc. Lives on `Pane` (not a separate
    /// `Vec` NimbleApp would have to keep in sync with `panes`) so it
    /// naturally follows spawn/close/duplicate without extra bookkeeping.
    monitor: PaneMonitor,
    /// Real, in-memory (see `history`'s own doc on why not persisted)
    /// browsing history - one entry per real navigation this pane's
    /// `BrowserView` was actually asked to perform.
    history: History,
    /// Real per-pane download list - see `downloads`'s own doc. Files
    /// land under `%TEMP%/nimble-downloads/<pane-id>/`.
    downloads: Downloads,
}

struct NimbleApp {
    panes: Vec<Pane>,
    /// Index into `panes` the address bar / proxy box / Reload button
    /// target - set by clicking a pane in the grid. Clamped back into
    /// range whenever the pane count shrinks.
    selected: usize,
    address_bar_text: String,
    /// The proxy text box's current contents - not necessarily what the
    /// selected pane is actually using, since a proxy only takes effect on
    /// [`apply_proxy`](Self::apply_proxy) (it can't be changed on an
    /// already-running profile - see `BrowserView::spawn_with_proxy`'s
    /// doc). Starts empty (no proxy).
    proxy_text: String,
    /// Named groups of profiles (mockup's workspace switcher). Real data
    /// layer, still no switcher UI here (see `workspace::WorkspaceManager`'s
    /// own doc) - its role in this GUI is naming each grid pane so an
    /// automation script can address it via `pane("pane-N")`.
    workspace: WorkspaceManager,
    automation_script: String,
    /// `Ok(result)` from the last automation script's top-level eval, or
    /// `Err(message)` if it raised. `None` before any script has run.
    automation_result: Option<Result<String, String>>,
    /// The active UI language - see `shell::i18n`. Toggled via the EN/PT
    /// buttons in the toolbar, applied immediately (every string is looked
    /// up fresh each `update()` frame, so there's no restart/re-render step
    /// needed).
    locale: Locale,
    /// The download-URL text box in the Downloads & History side panel -
    /// targets the selected pane on submit, same "selected pane" scoping
    /// the address bar/proxy box already use.
    download_url_text: String,
    performance: PerformanceSettings,
    /// `Some` once the Settings window has actually opened the real vault
    /// (`vault_ui::open`) - not eagerly at startup, so a session that
    /// never opens Settings never touches the filesystem for it.
    vault: Option<security::vault::CredentialVault>,
    vault_error: Option<String>,
    settings_open: bool,
    /// New-credential form fields in the Settings window's Credentials
    /// section.
    vault_key_text: String,
    vault_value_text: String,
}

const SPARKLINE_HEIGHT: f32 = 24.0;
const SPARKLINE_MARGIN: f32 = 6.0;

/// Draws the resource-monitor overlay (mockup's "CPU/RAM/FPS por profile,
/// gráfico 60s") in `cell_rect`'s top-left corner: one text line, plus a
/// hand-drawn CPU sparkline underneath it (no `egui_plot`/charting crate
/// dependency for one small line - `monitor.cpu_history()` is already
/// exactly the 0..=60-point series a sparkline needs). Draws nothing but
/// the text placeholder "..." before the monitor's first real sample
/// arrives (see `PaneMonitor::tick`'s throttling) rather than showing
/// stale zeros.
fn draw_resource_overlay(ui: &egui::Ui, cell_rect: egui::Rect, monitor: &PaneMonitor) {
    let text = match (monitor.latest_cpu_percent(), monitor.latest_memory_bytes(), monitor.latest_fps()) {
        (Some(cpu), Some(mem), Some(fps)) => format!("CPU {cpu:.0}% · RAM {:.0} MB · FPS {fps:.0}", mem as f64 / 1_000_000.0),
        _ => "CPU ... · RAM ... · FPS ...".to_string(),
    };
    let text_pos = cell_rect.min + egui::vec2(SPARKLINE_MARGIN, SPARKLINE_MARGIN);
    ui.painter().text(text_pos, egui::Align2::LEFT_TOP, text, egui::FontId::monospace(11.0), egui::Color32::WHITE);

    let history: Vec<f64> = monitor.cpu_history().collect();
    if history.len() < 2 {
        return;
    }
    let sparkline_rect = egui::Rect::from_min_size(
        text_pos + egui::vec2(0.0, 16.0),
        egui::vec2((cell_rect.width() - SPARKLINE_MARGIN * 2.0).max(0.0), SPARKLINE_HEIGHT),
    );
    ui.painter().rect_filled(sparkline_rect, 0.0, egui::Color32::from_black_alpha(120));
    let max = history.iter().cloned().fold(1.0_f64, f64::max); // at least 1.0 so an all-zero window doesn't divide by zero
    let points: Vec<egui::Pos2> = history
        .iter()
        .enumerate()
        .map(|(i, &cpu)| {
            let x = sparkline_rect.min.x + (i as f32 / (history.len() - 1) as f32) * sparkline_rect.width();
            let y = sparkline_rect.max.y - (cpu / max) as f32 * sparkline_rect.height();
            egui::pos2(x, y)
        })
        .collect();
    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.5_f32, egui::Color32::LIGHT_GREEN)));
}

/// Spawns one pane with the next positional id and registers it into the
/// active workspace.
fn spawn_pane(workspace: &mut WorkspaceManager, index: usize) -> Pane {
    let id = format!("pane-{}", index + 1);
    let active = workspace.active_index();
    workspace.add_profile(active, id.clone());
    let downloads_dir = std::env::temp_dir().join("nimble-downloads").join(&id);
    Pane {
        browser: BrowserView::spawn(PANE_WIDTH, PANE_HEIGHT),
        monitor: PaneMonitor::new(),
        history: History::new(),
        downloads: Downloads::new(downloads_dir),
        id,
    }
}

impl Default for NimbleApp {
    fn default() -> Self {
        let mut workspace = WorkspaceManager::new();
        let panes = vec![spawn_pane(&mut workspace, 0)];

        NimbleApp {
            panes,
            selected: 0,
            address_bar_text: String::new(),
            proxy_text: String::new(),
            workspace,
            automation_script: String::new(),
            automation_result: None,
            locale: Locale::En,
            download_url_text: String::new(),
            performance: PerformanceSettings::new(),
            vault: None,
            vault_error: None,
            settings_open: false,
            vault_key_text: String::new(),
            vault_value_text: String::new(),
        }
    }
}

impl NimbleApp {
    /// Indices into `self.panes` whose id is registered in the *active*
    /// workspace - what the grid actually draws, and what the "Panes:"
    /// count buttons grow/shrink. A pane belonging to a different
    /// workspace keeps running (its process isn't touched) but isn't
    /// visible while that other workspace isn't active - switching
    /// `WorkspaceManager`'s active index is what changes this list, not a
    /// respawn.
    fn active_workspace_pane_indices(&self) -> Vec<usize> {
        let active_ids = self.workspace.active().profiles();
        self.panes.iter().enumerate().filter(|(_, p)| active_ids.iter().any(|id| id == &p.id)).map(|(i, _)| i).collect()
    }

    /// Grows or shrinks the *active workspace's visible* pane count to
    /// exactly `count` (the mockup's 1/2/4/6 grid toggle - see
    /// `tiling::grid_layout`'s doc for why any other count still works).
    /// A newly grown pane is a genuinely new process, appended to
    /// `self.panes` and registered into the active workspace (same as
    /// [`spawn_pane`]). Shrinking closes the active workspace's own
    /// *trailing* panes (via [`close_pane`](Self::close_pane), largest
    /// index first so removal doesn't shift indices still to be closed
    /// out from under this loop) - panes belonging to other workspaces
    /// are never touched by this, regardless of count.
    fn set_pane_count(&mut self, count: usize) {
        let count = self.performance.clamp_pane_count(count);
        let visible = self.active_workspace_pane_indices();
        if visible.len() < count {
            for _ in visible.len()..count {
                let index = self.panes.len();
                self.panes.push(spawn_pane(&mut self.workspace, index));
            }
        } else if visible.len() > count {
            let mut to_close: Vec<usize> = visible[count..].to_vec();
            to_close.sort_unstable_by(|a, b| b.cmp(a)); // descending
            for index in to_close {
                self.close_pane(index);
            }
        }
    }

    fn navigate_to_address_bar(&mut self) {
        let url = self.address_bar_text.trim().to_string();
        if !url.is_empty() {
            let pane = &mut self.panes[self.selected];
            pane.browser.navigate(&url);
            pane.history.record(url);
        }
    }

    /// Respawns *the selected pane* with the proxy text box's current
    /// contents - an empty box means "no proxy". A running profile's own
    /// proxy is fixed for its process lifetime (matches `profile-worker`'s
    /// "set once at spawn" scope), so applying a change here is a real
    /// respawn of that one pane, not a live setting - its address bar and
    /// current page are lost, same as before this pane count existed.
    fn apply_proxy(&mut self) {
        let proxy = self.proxy_text.trim();
        let proxy = if proxy.is_empty() { None } else { Some(proxy) };
        self.panes[self.selected].browser = BrowserView::spawn_with_proxy(PANE_WIDTH, PANE_HEIGHT, proxy);
        // A respawn is a new OS process (see this method's own doc) - a
        // stale monitor would diff the new process's first sample against
        // the old process's last one, reading a nonsense CPU/FPS spike.
        self.panes[self.selected].monitor = PaneMonitor::new();
        self.address_bar_text.clear();
    }

    /// Runs `self.automation_script` once via [`automation_bridge::run_script`]
    /// against every live pane (not just the selected one - a script names
    /// whichever pane it wants via `pane("pane-N")`) and stores the result
    /// for the panel below to display.
    fn run_automation_script(&mut self) {
        let panes = self.panes.iter_mut().map(|p| (p.id.as_str(), &mut p.browser));
        self.automation_result = Some(automation_bridge::run_script(&self.workspace, panes, &self.automation_script));
    }

    /// Downloads `self.download_url_text` (real `net::download`, see
    /// `downloads::Downloads`) into the *selected* pane's own download
    /// directory, recording a real entry - success or failure - and
    /// clears the text box on submit either way (the failed attempt is
    /// still visible in the list below, no need to keep the URL sitting
    /// in the box).
    fn download_to_selected_pane(&mut self) {
        let url = self.download_url_text.trim().to_string();
        if url.is_empty() {
            return;
        }
        self.panes[self.selected].downloads.download(&url);
        self.download_url_text.clear();
    }

    /// Opens the real on-disk vault (`vault_ui::open`) if it isn't open
    /// yet - lazy, so a session that never opens Settings never touches
    /// the filesystem for this. A failure (permissions, corrupted vault
    /// file) is stored in `self.vault_error` and shown in the Settings
    /// window rather than silently leaving the Credentials section blank.
    fn ensure_vault_open(&mut self) {
        if self.vault.is_some() {
            return;
        }
        match vault_ui::open(&vault_ui::default_vault_dir()) {
            Ok(vault) => {
                self.vault = Some(vault);
                self.vault_error = None;
            }
            Err(e) => self.vault_error = Some(e.to_string()),
        }
    }

    /// Adds `self.vault_key_text` -> `self.vault_value_text` to the real
    /// vault (encrypts + flushes to disk immediately, see
    /// `CredentialVault::set`'s own doc) and clears both fields on
    /// success.
    fn add_credential(&mut self) {
        let key = self.vault_key_text.trim().to_string();
        if key.is_empty() {
            return;
        }
        let Some(vault) = self.vault.as_mut() else { return };
        match vault.set(&key, &self.vault_value_text) {
            Ok(()) => {
                self.vault_key_text.clear();
                self.vault_value_text.clear();
                self.vault_error = None;
            }
            Err(e) => self.vault_error = Some(e.to_string()),
        }
    }

    fn remove_credential(&mut self, key: &str) {
        let Some(vault) = self.vault.as_mut() else { return };
        if let Err(e) = vault.remove(key) {
            self.vault_error = Some(e.to_string());
        }
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
    fn run_automation_script_for_pane(&mut self, index: usize) {
        let pane = &mut self.panes[index];
        let panes = std::iter::once((pane.id.as_str(), &mut pane.browser));
        self.automation_result = Some(automation_bridge::run_script(&self.workspace, panes, &self.automation_script));
    }

    /// Removes exactly `self.panes[index]` (not just the trailing pane -
    /// unlike [`set_pane_count`](Self::set_pane_count)'s grow/shrink,
    /// which only ever pops the end). Force-kills that pane's process the
    /// same way any other pane removal does (`Pane`'s `BrowserView` ->
    /// `profile::Profile`'s own real `Drop`). Refuses to remove the last
    /// remaining pane - a shell with zero panes has nothing to show and no
    /// way to add one back without a "new pane" control this pass doesn't
    /// add (mirrors `WorkspaceManager::remove`'s own "never zero" refusal).
    fn close_pane(&mut self, index: usize) {
        if self.panes.len() <= 1 {
            return;
        }
        let pane = self.panes.remove(index);
        let active = self.workspace.active_index();
        self.workspace.remove_profile(active, &pane.id);
        if self.selected >= self.panes.len() {
            self.selected = self.panes.len() - 1;
        } else if self.selected > index {
            self.selected -= 1;
        }
    }

    /// Spawns a fresh pane and navigates it to `self.panes[index]`'s
    /// current URL - "duplicate" in the sense of "another pane on the same
    /// page", not a real session clone: each profile gets its own
    /// `storage_root` keyed by a fresh shmem name (see `profile-worker`'s
    /// own doc), so cookies/`localStorage`/login state are NOT copied.
    /// Real account cloning would need to duplicate that storage
    /// directory, which this doesn't do - documented here rather than
    /// silently producing a pane that looks duplicated but isn't logged
    /// in.
    fn duplicate_pane(&mut self, index: usize) {
        let url = self.panes[index].browser.current_url().to_string();
        let new_index = self.panes.len();
        let mut pane = spawn_pane(&mut self.workspace, new_index);
        if !url.is_empty() {
            pane.browser.navigate(&url);
            pane.history.record(url);
        }
        self.panes.push(pane);
    }

    /// Moves `self.panes[index]`'s id to the workspace at `workspace_index`
    /// via the real, already-tested `WorkspaceManager::move_profile`. The
    /// pane keeps running exactly as before - only which workspace lists
    /// its id changes, which in turn changes whether an automation script
    /// targeting the *other* workspace can still address it (see
    /// `automation_bridge::run_script`'s "only what the active workspace
    /// lists" scoping).
    fn move_pane_to_workspace(&mut self, index: usize, workspace_index: usize) {
        self.workspace.move_profile(&self.panes[index].id, workspace_index);
    }

    /// Creates a new workspace (named positionally, "Workspace 2", "Workspace
    /// 3", ...) and moves `self.panes[index]` into it in one step - the
    /// context menu's "Move to workspace... > New workspace" action. No
    /// text-input prompt for a custom name in this pass (egui has no
    /// blocking dialog primitive here) - renaming a workspace after
    /// creation isn't wired into this GUI at all yet.
    fn move_pane_to_new_workspace(&mut self, index: usize) {
        let name = format!("Workspace {}", self.workspace.workspaces().len() + 1);
        let new_index = self.workspace.create(name);
        self.move_pane_to_workspace(index, new_index);
    }

    /// Switches which workspace is active - the grid now shows only
    /// `self.panes` whose id that workspace lists (see
    /// `active_workspace_pane_indices`'s doc), all other panes keep
    /// running unseen. Re-clamps `self.selected` immediately (not left for
    /// the next `CentralPanel` frame) since the toolbar's Reload/address
    /// bar/proxy controls run before the grid in `update`'s draw order and
    /// would otherwise act on a pane this frame no longer shows.
    fn switch_workspace(&mut self, index: usize) {
        self.workspace.set_active(index);
        let visible = self.active_workspace_pane_indices();
        self.selected = visible.first().copied().unwrap_or(0);
    }

    /// The toolbar's "+ New" workspace button: creates an empty workspace
    /// (positionally named, same as [`move_pane_to_new_workspace`](Self::move_pane_to_new_workspace))
    /// and switches to it - the grid shows nothing until the pane-count
    /// buttons spawn one (which registers into whichever workspace is now
    /// active) or a pane is moved in from elsewhere via its context menu.
    fn create_workspace_and_switch(&mut self) {
        let name = format!("Workspace {}", self.workspace.workspaces().len() + 1);
        let new_index = self.workspace.create(name);
        self.switch_workspace(new_index);
    }
}

impl eframe::App for NimbleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Every pane's process keeps rendering on its own real vsync loop
        // regardless of whether this GUI repaints - poll on a matching
        // cadence rather than only reacting to user input, or newly
        // published frames would sit unseen between interactions.
        ctx.request_repaint_after(Duration::from_millis(16));

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Workspace:");
                let active_index = self.workspace.active_index();
                let mut switch_to = None;
                for (i, workspace) in self.workspace.workspaces().iter().enumerate() {
                    if ui.selectable_label(i == active_index, &workspace.name).clicked() {
                        switch_to = Some(i);
                    }
                }
                let new_clicked = ui.button("+ New").clicked();
                if let Some(i) = switch_to {
                    self.switch_workspace(i);
                }
                if new_clicked {
                    self.create_workspace_and_switch();
                }
            });
            let visible = self.active_workspace_pane_indices();
            ui.horizontal(|ui| {
                ui.label(i18n::t(i18n::PANES_LABEL, self.locale));
                for count in [1, 2, 4, 6] {
                    let allowed = count <= self.performance.max_panes;
                    if ui.add_enabled(allowed, egui::SelectableLabel::new(visible.len() == count, count.to_string())).clicked() {
                        self.set_pane_count(count);
                    }
                }
                if ui.button("⚙ Settings").clicked() {
                    self.settings_open = !self.settings_open;
                }
                ui.separator();
                if ui.selectable_label(self.locale == Locale::En, "EN").clicked() {
                    self.locale = Locale::En;
                }
                if ui.selectable_label(self.locale == Locale::Pt, "PT").clicked() {
                    self.locale = Locale::Pt;
                }
                ui.separator();
                // `visible` (this workspace's panes) can be empty right
                // after switching to a freshly created workspace -
                // `self.selected` stays a valid `self.panes` index either
                // way (that invariant is `close_pane`/`switch_workspace`'s
                // job), but showing/acting on it here would be confusing
                // when the grid below isn't even displaying it.
                if !visible.is_empty() {
                    ui.label(i18n::fill(i18n::t(i18n::SELECTED_LABEL, self.locale), &[&self.panes[self.selected].id]));
                    if ui.button(i18n::t(i18n::RELOAD_BUTTON, self.locale)).clicked() {
                        self.panes[self.selected].browser.reload();
                    }
                }
            });
            if visible.is_empty() {
                ui.label("No panes in this workspace - use the pane count buttons above to spawn one, or move one in from another pane's context menu.");
                return;
            }
            ui.horizontal(|ui| {
                let address_bar = ui.add_sized(
                    [ui.available_width() - 8.0, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut self.address_bar_text).hint_text(i18n::t(i18n::ADDRESS_BAR_HINT, self.locale)),
                );
                if address_bar.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.navigate_to_address_bar();
                }
            });
            ui.horizontal(|ui| {
                ui.label(i18n::t(i18n::PROXY_LABEL, self.locale));
                let proxy_field = ui.add_sized(
                    [220.0, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut self.proxy_text).hint_text(i18n::t(i18n::PROXY_HINT, self.locale)),
                );
                let apply_clicked = ui.button(i18n::t(i18n::APPLY_BUTTON, self.locale)).clicked();
                let applied_via_enter = proxy_field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if apply_clicked || applied_via_enter {
                    self.apply_proxy();
                }
            });
            let selected = &self.panes[self.selected].browser;
            if let Some(error) = selected.error() {
                ui.colored_label(egui::Color32::RED, error);
            } else if let Some(error) = selected.navigation_error() {
                let prefix = i18n::t(i18n::FAILED_TO_LOAD_PREFIX, self.locale);
                ui.colored_label(egui::Color32::RED, format!("{prefix} {}: {error}", selected.current_url()));
            }
        });

        egui::TopBottomPanel::bottom("automation").show(ctx, |ui| {
            let pane_ids = self.panes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>().join(", ");
            ui.label(i18n::fill(i18n::t(i18n::AUTOMATION_HEADER, self.locale), &[&self.workspace.active().name, &pane_ids]));
            ui.add(
                egui::TextEdit::multiline(&mut self.automation_script)
                    .hint_text(i18n::t(i18n::AUTOMATION_SCRIPT_HINT, self.locale))
                    .desired_rows(3),
            );
            if ui.button(i18n::t(i18n::RUN_BUTTON, self.locale)).clicked() {
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

        if self.settings_open {
            self.ensure_vault_open();
            let mut open = self.settings_open;
            egui::Window::new("Settings").open(&mut open).show(ctx, |ui| {
                ui.heading("Performance");
                ui.add(egui::Slider::new(&mut self.performance.max_panes, 1..=6).text("Max live panes"));
                ui.label("Background throttling / GPU selection / per-pane frame cap: not implemented yet - profile-worker's vsync loop has no command for any of those (fixed TARGET_FPS since spawn).");

                ui.separator();
                ui.heading("Credentials");
                ui.label("Real AES-256-GCM encrypted vault (security::CredentialVault) - not the real OS keychain yet, see that module's doc.");
                if let Some(error) = &self.vault_error {
                    ui.colored_label(egui::Color32::RED, error);
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.vault_key_text).hint_text("key (e.g. login.email)"));
                    ui.add(egui::TextEdit::singleline(&mut self.vault_value_text).hint_text("value").password(true));
                    if ui.button("Add").clicked() {
                        self.add_credential();
                    }
                });
                ui.separator();
                let mut to_remove: Option<String> = None;
                if let Some(vault) = &self.vault {
                    let keys: Vec<String> = vault.keys().map(str::to_string).collect();
                    if keys.is_empty() {
                        ui.weak("No stored credentials.");
                    }
                    for key in keys {
                        ui.horizontal(|ui| {
                            ui.label(&key);
                            if ui.small_button("Remove").clicked() {
                                to_remove = Some(key);
                            }
                        });
                    }
                }
                if let Some(key) = to_remove {
                    self.remove_credential(&key);
                }
            });
            self.settings_open = open;
        }

        egui::SidePanel::right("downloads_history").resizable(true).default_width(260.0).show(ctx, |ui| {
            let visible = self.active_workspace_pane_indices();
            if visible.is_empty() {
                ui.label("No panes in this workspace.");
                return;
            }
            ui.heading(format!("Downloads & History — {}", self.panes[self.selected].id));

            ui.horizontal(|ui| {
                let url_box = ui.add_sized(
                    [ui.available_width() - 70.0, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut self.download_url_text).hint_text("URL to download"),
                );
                let clicked = ui.button("Download").clicked();
                if clicked || (url_box.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    self.download_to_selected_pane();
                }
            });

            ui.separator();
            ui.label("Downloads");
            egui::ScrollArea::vertical().id_source("downloads_list").max_height(160.0).show(ui, |ui| {
                let pane = &self.panes[self.selected];
                if pane.downloads.is_empty() {
                    ui.weak("No downloads yet.");
                }
                for record in pane.downloads.entries() {
                    match &record.result {
                        Ok(bytes) => {
                            ui.label(format!("{} ({bytes} bytes) <- {}", record.dest.display(), record.url));
                        }
                        Err(error) => {
                            ui.colored_label(egui::Color32::RED, format!("{} failed: {error}", record.url));
                        }
                    }
                }
            });

            ui.separator();
            ui.label("History");
            egui::ScrollArea::vertical().id_source("history_list").show(ui, |ui| {
                let pane = &self.panes[self.selected];
                if pane.history.is_empty() {
                    ui.weak("No history yet.");
                }
                for entry in pane.history.entries() {
                    ui.label(&entry.url);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_rect_before_wrap();
            let container = tiling::Rect { x: available.min.x, y: available.min.y, width: available.width(), height: available.height() };
            // Only the active workspace's panes - see
            // `active_workspace_pane_indices`'s doc. A pane belonging to a
            // different (inactive) workspace keeps running but isn't
            // drawn or ticked here.
            let visible = self.active_workspace_pane_indices();
            let cells = tiling::grid_layout(container, visible.len());

            // Deliberately not a `for (pane, cell) in self.panes.iter_mut()...`
            // loop: the context menu below needs `&self.workspace` (to list
            // move-to-workspace targets) at the same time other branches
            // need `&mut self` (close/duplicate/move a pane) - an
            // `iter_mut()` borrow spanning the whole loop body would
            // conflict with those `self.method(...)` calls. Indexing
            // `self.panes[index]` fresh each time avoids holding a borrow
            // across them.
            for (slot, cell) in cells.into_iter().enumerate() {
                let index = visible[slot];
                let cell_rect = egui::Rect::from_min_size(egui::pos2(cell.x, cell.y), egui::vec2(cell.width, cell.height));
                let mut cell_ui = ui.child_ui(cell_rect, egui::Layout::top_down(egui::Align::Center), None);

                let response = cell_ui.allocate_response(cell_rect.size(), egui::Sense::click());
                if response.clicked() {
                    self.selected = index;
                }

                let mut close_clicked = false;
                let mut duplicate_clicked = false;
                let mut reload_clicked = false;
                let mut run_here_clicked = false;
                let mut move_to: Option<usize> = None;
                let mut move_to_new = false;
                let workspaces = self.workspace.workspaces();
                response.context_menu(|ui| {
                    if ui.button("Reload").clicked() {
                        reload_clicked = true;
                        ui.close_menu();
                    }
                    if ui.button("Duplicate profile").clicked() {
                        duplicate_clicked = true;
                        ui.close_menu();
                    }
                    if ui.button("Run auto login (shared script)").clicked() {
                        run_here_clicked = true;
                        ui.close_menu();
                    }
                    // Web Audio exists now (js_runtime::web_audio -
                    // real OfflineAudioContext synthesis/mixing), but
                    // there's still no live AudioContext or OS audio
                    // output device anywhere in this engine - only
                    // headless rendering into an in-memory buffer. A
                    // "mute" toggle needs something actually playing
                    // sound to mute; still disabled, not faked.
                    ui.add_enabled(false, egui::Button::new("Mute audio")).on_disabled_hover_text("not implemented - no live audio output exists yet (only OfflineAudioContext's headless rendering)");
                    ui.menu_button("Move to workspace", |ui| {
                        for (workspace_index, workspace) in workspaces.iter().enumerate() {
                            if ui.button(&workspace.name).clicked() {
                                move_to = Some(workspace_index);
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui.button("New workspace...").clicked() {
                            move_to_new = true;
                            ui.close_menu();
                        }
                    });
                    // Real inspector doesn't exist in any phase of this
                    // engine yet - the spec itself flags "dev tools" as an
                    // open gap (CLAUDE.md), not something to fake here.
                    ui.add_enabled(false, egui::Button::new("Dev tools")).on_disabled_hover_text("not implemented - no inspector exists in this engine yet");
                    ui.separator();
                    if ui.button("Close pane").clicked() {
                        close_clicked = true;
                        ui.close_menu();
                    }
                });

                if reload_clicked {
                    self.panes[index].browser.reload();
                }
                if run_here_clicked {
                    self.run_automation_script_for_pane(index);
                }
                if let Some(workspace_index) = move_to {
                    self.move_pane_to_workspace(index, workspace_index);
                }
                if move_to_new {
                    self.move_pane_to_new_workspace(index);
                }
                // Structural changes (pane count changes) invalidate
                // `cells`, which was computed for the pane count at the
                // top of this closure - stop drawing the rest of this
                // frame's grid rather than index a now-stale layout. Only
                // costs one frame; the next repaint (16ms later, see the
                // `request_repaint_after` above) re-lays-out from scratch.
                if duplicate_clicked {
                    self.duplicate_pane(index);
                    break;
                }
                if close_clicked {
                    self.close_pane(index);
                    break;
                }

                let pane = &mut self.panes[index];
                if let Some(texture) = pane.browser.poll_texture(ctx) {
                    let image_size = texture.size_vec2();
                    let scale = (cell_rect.width() / image_size.x).min(cell_rect.height() / image_size.y).min(1.0);
                    cell_ui.put(cell_rect, egui::Image::new((texture.id(), image_size * scale)));
                } else if pane.browser.error().is_none() {
                    cell_ui.put(cell_rect, egui::Label::new(i18n::t(i18n::STARTING_PROFILE, self.locale)));
                }

                if let (Some(pid), Some(frame_generation)) = (pane.browser.pid(), pane.browser.frame_generation()) {
                    pane.monitor.tick(pid, frame_generation);
                }
                draw_resource_overlay(&cell_ui, cell_rect, &pane.monitor);

                let border_color = if index == self.selected { egui::Color32::LIGHT_BLUE } else { egui::Color32::DARK_GRAY };
                cell_ui.painter().rect_stroke(cell_rect, 0.0, egui::Stroke::new(2.0_f32, border_color));
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native("Nimble", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(NimbleApp::default()))))
}
