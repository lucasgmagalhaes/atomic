//! The mockup's "Add Profile" modal window.

use super::NimbleApp;

impl NimbleApp {
  pub(super) fn draw_add_profile_modal(&mut self, ctx: &egui::Context) {
    if self.add_profile_form.is_none() {
      return;
    }
    let mut open = true;
    egui::Window::new("Add Profile").open(&mut open).show(ctx, |ui| {
            let form = self.add_profile_form.as_mut().expect("checked is_some above");
            egui::Grid::new("add_profile_form_grid").num_columns(2).show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut form.name);
                ui.end_row();

                ui.label("Start URL");
                ui.text_edit_singleline(&mut form.start_url);
                ui.end_row();

                ui.label("Email");
                ui.text_edit_singleline(&mut form.email);
                ui.end_row();

                ui.label("Password");
                ui.add(egui::TextEdit::singleline(&mut form.password).password(true));
                ui.end_row();

                ui.label("Proxy");
                ui.add(egui::TextEdit::singleline(&mut form.proxy).hint_text("host:port or user:pass@host:port"));
                ui.end_row();
            });
            ui.weak("\"Launch on start\" isn't implemented - no shell restart persists which profiles existed yet.");
            if let Some(error) = &self.add_profile_error {
                ui.colored_label(egui::Color32::RED, error);
            }
            ui.horizontal(|ui| {
                if ui.button("Create").clicked() {
                    self.create_profile();
                }
                if ui.button("Cancel").clicked() {
                    self.add_profile_form = None;
                    self.add_profile_error = None;
                }
            });
        });
    if !open {
      self.add_profile_form = None;
      self.add_profile_error = None;
    }
  }
}
