//! Track B spike: draws the toolbar rendered by `crate::chrome_engine`
//! (Nimble's own HTML/CSS/JS engine) as an egui texture, right below the
//! real `draw_toolbar` — additive, not a replacement, so today's working
//! toolbar keeps working while this proves the architecture. Regenerates
//! the texture every frame (fine for a small, low-frequency-repaint
//! toolbar at spike scale; a real migration would cache by the chrome
//! engine's own DOM mutation/layout version, same as `Page::layout`'s
//! cache).

use super::NimbleApp;

const CHROME_HEIGHT: u32 = 40;

impl NimbleApp {
    pub(super) fn draw_chrome_toolbar_spike(&mut self, ctx: &egui::Context) {
        let workspaces: Vec<(String, bool)> = {
            let active = self.workspace.active_index();
            self.workspace
                .workspaces()
                .iter()
                .enumerate()
                .map(|(i, w)| (w.name.clone(), i == active))
                .collect()
        };
        self.chrome_toolbar
            .sync_toolbar_state(&workspaces, self.locale == shell::i18n::Locale::En);

        egui::TopBottomPanel::top("chrome_toolbar_spike").show(ctx, |ui| {
            let width = ui.available_width().max(1.0) as u32;
            let pixels = self
                .chrome_toolbar
                .render(&self.chrome_gpu, width, CHROME_HEIGHT);
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [width as usize, CHROME_HEIGHT as usize],
                &pixels,
            );
            let texture = self.chrome_texture.get_or_insert_with(|| {
                ctx.load_texture(
                    "chrome-toolbar",
                    image.clone(),
                    egui::TextureOptions::LINEAR,
                )
            });
            texture.set(image, egui::TextureOptions::LINEAR);

            let response = ui.add(
                egui::Image::new((texture.id(), texture.size_vec2())).sense(egui::Sense::click()),
            );
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let local = pos - response.rect.min;
                    self.chrome_toolbar
                        .handle_click(width, local.x as f64, local.y as f64);
                }
            }
            // Real live :hover — applied from this frame's pointer
            // position so it's visible next frame (the render() call
            // above already happened for this frame; one-frame latency
            // is the same immediate-mode-UI tradeoff egui's own hover
            // effects already have). An off-screen coordinate when the
            // pointer isn't over the image at all hit-tests to nothing,
            // which is exactly "clear hover".
            let hover_local = response
                .hover_pos()
                .map(|pos| pos - response.rect.min)
                .unwrap_or(egui::Vec2::new(-1.0, -1.0));
            self.chrome_toolbar
                .update_hover(width, hover_local.x as f64, hover_local.y as f64);

            for action in self.chrome_toolbar.drain_actions() {
                use crate::chrome_bridge::ChromeAction;
                match action {
                    ChromeAction::AddProfile => self.open_add_profile_modal(),
                    ChromeAction::SetPaneCount(n) => self.set_pane_count(n),
                    ChromeAction::SwitchWorkspace(i) => self.switch_workspace(i),
                    ChromeAction::CreateWorkspace => self.create_workspace_and_switch(),
                    ChromeAction::SetLocale(locale) => {
                        self.locale = if locale == "pt" {
                            shell::i18n::Locale::Pt
                        } else {
                            shell::i18n::Locale::En
                        };
                    }
                    // Every other variant belongs to a different chrome
                    // surface (downloads/history, add-profile, settings) —
                    // `ChromeAction` stays one enum shared by all of them,
                    // so each surface's own drain match only handles its
                    // own and ignores the rest.
                    _ => {}
                }
            }
        });
    }
}
