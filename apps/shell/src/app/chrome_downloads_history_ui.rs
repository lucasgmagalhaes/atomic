//! Track B: the Downloads & History panel rendered by
//! `crate::chrome_engine` instead of egui — same pattern
//! `chrome_toolbar_ui.rs` already established, additive next to the real
//! `draw_downloads_history_panel` for now.

use super::NimbleApp;

const CHROME_WIDTH: u32 = 260;
const CHROME_HEIGHT: u32 = 400;

impl NimbleApp {
    pub(super) fn draw_chrome_downloads_history_spike(&mut self, ctx: &egui::Context) {
        let visible = self.active_workspace_pane_indices();
        if visible.is_empty() {
            return;
        }

        let (downloads, history): (Vec<String>, Vec<String>) = {
            let pane = &self.panes[self.selected];
            let downloads = pane
                .downloads
                .entries()
                .map(|record| match &record.result {
                    Ok(bytes) => format!(
                        "{} ({bytes} bytes) <- {}",
                        record.dest.display(),
                        record.url
                    ),
                    Err(error) => format!("{} failed: {error}", record.url),
                })
                .collect();
            let history = pane
                .history
                .entries()
                .map(|entry| entry.url.clone())
                .collect();
            (downloads, history)
        };
        self.chrome_downloads_history
            .sync_downloads_history(&downloads, &history);

        egui::SidePanel::right("chrome_downloads_history_spike")
            .resizable(true)
            .default_width(CHROME_WIDTH as f32)
            .show(ctx, |ui| {
                let width = ui.available_width().max(1.0) as u32;
                let pixels =
                    self.chrome_downloads_history
                        .render(&self.chrome_gpu, width, CHROME_HEIGHT);
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [width as usize, CHROME_HEIGHT as usize],
                    &pixels,
                );
                let texture = self.chrome_downloads_texture.get_or_insert_with(|| {
                    ctx.load_texture(
                        "chrome-downloads-history",
                        image.clone(),
                        egui::TextureOptions::LINEAR,
                    )
                });
                texture.set(image, egui::TextureOptions::LINEAR);

                let response = ui.add(
                    egui::Image::new((texture.id(), texture.size_vec2()))
                        .sense(egui::Sense::click()),
                );
                if response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let local = pos - response.rect.min;
                        let (x, y) = (local.x as f64, local.y as f64);
                        if let Some(id) = self
                            .chrome_downloads_history
                            .resolve_click_target(width, x, y)
                        {
                            // Read the input's live value *before*
                            // dispatching — `downloads_history.js`'s own
                            // click handler clears it right after, so
                            // reading afterward would always see "".
                            if id == "download-submit" {
                                if let Some(url) =
                                    self.chrome_downloads_history.input_value("download-url")
                                {
                                    if !url.trim().is_empty() {
                                        self.download_url_text = url;
                                        self.download_to_selected_pane();
                                    }
                                }
                            }
                            self.chrome_downloads_history.dispatch_click(&id);
                        }
                    }
                }

                // Real live :hover — same one-frame-latency tradeoff
                // `chrome_toolbar_ui.rs` documents for its own hover wiring.
                let hover_local = response
                    .hover_pos()
                    .map(|pos| pos - response.rect.min)
                    .unwrap_or(egui::Vec2::new(-1.0, -1.0));
                self.chrome_downloads_history.update_hover(
                    width,
                    hover_local.x as f64,
                    hover_local.y as f64,
                );

                for action in self.chrome_downloads_history.drain_actions() {
                    // This surface never actually queues one of these
                    // itself (the submit path above handles it directly,
                    // synchronously, before the click even dispatches) —
                    // matched here only so `ChromeAction` stays one enum
                    // shared by every chrome surface.
                    let _ = action;
                }
            });
    }
}
