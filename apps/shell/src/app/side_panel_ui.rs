//! The Downloads & History side panel for the selected pane.

use super::NimbleApp;

impl NimbleApp {
    pub(super) fn draw_downloads_history_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("downloads_history")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                let visible = self.active_workspace_pane_indices();
                if visible.is_empty() {
                    ui.label("No panes in this workspace.");
                    return;
                }
                ui.heading(format!(
                    "Downloads & History — {}",
                    self.panes[self.selected].id
                ));

                ui.horizontal(|ui| {
                    let url_box = ui.add_sized(
                        [ui.available_width() - 70.0, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut self.download_url_text)
                            .hint_text("URL to download"),
                    );
                    let clicked = ui.button("Download").clicked();
                    if clicked
                        || (url_box.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        self.download_to_selected_pane();
                    }
                });

                ui.separator();
                ui.label("Downloads");
                egui::ScrollArea::vertical()
                    .id_source("downloads_list")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        let pane = &self.panes[self.selected];
                        if pane.downloads.is_empty() {
                            ui.weak("No downloads yet.");
                        }
                        for record in pane.downloads.entries() {
                            match &record.result {
                                Ok(bytes) => {
                                    ui.label(format!(
                                        "{} ({bytes} bytes) <- {}",
                                        record.dest.display(),
                                        record.url
                                    ));
                                }
                                Err(error) => {
                                    ui.colored_label(
                                        egui::Color32::RED,
                                        format!("{} failed: {error}", record.url),
                                    );
                                }
                            }
                        }
                    });

                ui.separator();
                ui.label("History");
                egui::ScrollArea::vertical()
                    .id_source("history_list")
                    .show(ui, |ui| {
                        let pane = &self.panes[self.selected];
                        if pane.history.is_empty() {
                            ui.weak("No history yet.");
                        }
                        for entry in pane.history.entries() {
                            ui.label(&entry.url);
                        }
                    });
            });
    }
}
