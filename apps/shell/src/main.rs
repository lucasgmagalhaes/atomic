use std::time::Duration;

use shell::automation_bridge;
use shell::browser_view::BrowserView;
use shell::tiling;
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
}

/// Spawns one pane with the next positional id and registers it into the
/// active workspace.
fn spawn_pane(workspace: &mut WorkspaceManager, index: usize) -> Pane {
    let id = format!("pane-{}", index + 1);
    let active = workspace.active_index();
    workspace.add_profile(active, id.clone());
    Pane { id, browser: BrowserView::spawn(PANE_WIDTH, PANE_HEIGHT) }
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
        }
    }
}

impl NimbleApp {
    /// Grows or shrinks `self.panes` to exactly `count` (the mockup's
    /// 1/2/4/6 grid toggle - see `tiling::grid_layout`'s doc for why any
    /// other count still works). Shrinking drops the trailing panes -
    /// `Pane`'s `BrowserView` (and therefore its `profile::Profile`, which
    /// has its own real `Drop` impl - see that crate) is force-killed the
    /// same way closing a single pane always was, not a new code path.
    /// Every remaining pane keeps its id and live process; only newly
    /// grown ones spawn fresh.
    fn set_pane_count(&mut self, count: usize) {
        let count = count.max(1);
        while self.panes.len() < count {
            let index = self.panes.len();
            self.panes.push(spawn_pane(&mut self.workspace, index));
        }
        while self.panes.len() > count {
            if let Some(pane) = self.panes.pop() {
                let active = self.workspace.active_index();
                self.workspace.remove_profile(active, &pane.id);
            }
        }
        if self.selected >= self.panes.len() {
            self.selected = self.panes.len() - 1;
        }
    }

    fn navigate_to_address_bar(&mut self) {
        let url = self.address_bar_text.trim().to_string();
        if !url.is_empty() {
            self.panes[self.selected].browser.navigate(&url);
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
                ui.label("Panes:");
                for count in [1, 2, 4, 6] {
                    if ui.selectable_label(self.panes.len() == count, count.to_string()).clicked() {
                        self.set_pane_count(count);
                    }
                }
                ui.separator();
                ui.label(format!("Selected: {}", self.panes[self.selected].id));
                if ui.button("Reload").clicked() {
                    self.panes[self.selected].browser.reload();
                }
            });
            ui.horizontal(|ui| {
                let address_bar = ui.add_sized(
                    [ui.available_width() - 8.0, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut self.address_bar_text).hint_text("Enter a URL for the selected pane and press Enter"),
                );
                if address_bar.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.navigate_to_address_bar();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Proxy (selected pane):");
                let proxy_field = ui.add_sized(
                    [220.0, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut self.proxy_text).hint_text("host:port (empty = none)"),
                );
                let apply_clicked = ui.button("Apply").clicked();
                let applied_via_enter = proxy_field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if apply_clicked || applied_via_enter {
                    self.apply_proxy();
                }
            });
            let selected = &self.panes[self.selected].browser;
            if let Some(error) = selected.error() {
                ui.colored_label(egui::Color32::RED, error);
            } else if let Some(error) = selected.navigation_error() {
                ui.colored_label(egui::Color32::RED, format!("Failed to load {}: {error}", selected.current_url()));
            }
        });

        egui::TopBottomPanel::bottom("automation").show(ctx, |ui| {
            ui.label(format!(
                "Automation — pane names from the active workspace (\"{}\"): {}",
                self.workspace.active().name,
                self.panes.iter().map(|p| p.id.as_str()).collect::<Vec<_>>().join(", ")
            ));
            ui.add(
                egui::TextEdit::multiline(&mut self.automation_script)
                    .hint_text(r#"pane("pane-1").goto("https://example.com")"#)
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
            let available = ui.available_rect_before_wrap();
            let container = tiling::Rect { x: available.min.x, y: available.min.y, width: available.width(), height: available.height() };
            let cells = tiling::grid_layout(container, self.panes.len());

            // Deliberately not a `for (pane, cell) in self.panes.iter_mut()...`
            // loop: the context menu below needs `&self.workspace` (to list
            // move-to-workspace targets) at the same time other branches
            // need `&mut self` (close/duplicate/move a pane) - an
            // `iter_mut()` borrow spanning the whole loop body would
            // conflict with those `self.method(...)` calls. Indexing
            // `self.panes[index]` fresh each time avoids holding a borrow
            // across them.
            for (index, cell) in cells.into_iter().enumerate() {
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
                    // No audio pipeline exists anywhere in this engine (no
                    // `<audio>`/`<video>`/Web Audio - see the phase notes
                    // in CLAUDE.md) - disabled rather than faking a mute
                    // toggle that would have nothing to actually mute.
                    ui.add_enabled(false, egui::Button::new("Mute audio")).on_disabled_hover_text("not implemented - this engine has no audio pipeline yet");
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
                    cell_ui.put(cell_rect, egui::Label::new("Starting profile process..."));
                }

                let border_color = if index == self.selected { egui::Color32::LIGHT_BLUE } else { egui::Color32::DARK_GRAY };
                cell_ui.painter().rect_stroke(cell_rect, 0.0, egui::Stroke::new(2.0_f32, border_color));
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    eframe::run_native("Nimble", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(NimbleApp::default()))))
}
