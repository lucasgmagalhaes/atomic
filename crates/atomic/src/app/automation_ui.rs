//! The bottom automation script box: text editor, Run button, and the
//! last run's result.

use atomic::i18n;

use super::AtomicApp;

impl AtomicApp {
    pub(super) fn draw_automation_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("automation").show(ui, |ui| {
            let pane_ids = self
                .panes
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            ui.label(i18n::fill(
                i18n::t(i18n::AUTOMATION_HEADER, self.locale),
                &[&self.workspace.active().name, &pane_ids],
            ));
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
    }
}
