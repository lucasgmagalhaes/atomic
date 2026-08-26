//! The top toolbar: workspace switcher, pane-count buttons, Settings/Add
//! Profile buttons, locale toggle, address bar, and proxy box.

use shell::i18n::{self, Locale};

use super::NimbleApp;

impl NimbleApp {
  pub(super) fn draw_toolbar(&mut self, ctx: &egui::Context) {
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
                if ui.button("+ Add Profile").clicked() {
                    self.open_add_profile_modal();
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
  }
}
