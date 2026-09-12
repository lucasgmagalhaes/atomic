//! Track B: the Add Profile modal rendered by `crate::chrome_engine`
//! instead of egui — same pattern the toolbar/downloads-history spikes
//! already established, additive next to the real `draw_add_profile_modal`
//! for now. The one new wrinkle relative to those two: real text input,
//! via `ChromeEngine::click_at_with_focus`/`type_key` (keyboard routing
//! follows `grid_ui.rs`'s existing real-pane pattern almost verbatim).

use super::NimbleApp;

const CHROME_WIDTH: u32 = 320;
const CHROME_HEIGHT: u32 = 220;

impl NimbleApp {
    pub(super) fn draw_chrome_add_profile_spike(&mut self, ctx: &egui::Context) {
        let Some(form) = self.add_profile_form.as_ref() else {
            self.chrome_add_profile_was_open = false;
            return;
        };

        if !self.chrome_add_profile_was_open {
            // Freshly opened this frame — prefill/reset every field once,
            // same as the real modal building a fresh `AddProfileForm`.
            // Not done every frame: that would fight with what the user is
            // actively typing.
            self.chrome_add_profile.set_input_values(&[
                ("field-name", &form.name),
                ("field-start-url", &form.start_url),
                ("field-email", &form.email),
                ("field-password", &form.password),
                ("field-proxy", &form.proxy),
            ]);
            self.chrome_add_profile_was_open = true;
        }
        self.chrome_add_profile
            .set_error_text(self.add_profile_error.as_deref().unwrap_or(""));

        egui::Window::new("chrome_add_profile_spike")
            .title_bar(false)
            .show(ctx, |ui| {
                let width = CHROME_WIDTH;
                let pixels = self
                    .chrome_add_profile
                    .render(&self.chrome_gpu, width, CHROME_HEIGHT);
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [width as usize, CHROME_HEIGHT as usize],
                    &pixels,
                );
                let texture = self.chrome_add_profile_texture.get_or_insert_with(|| {
                    ctx.load_texture(
                        "chrome-add-profile",
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
                    // Without this, `response.has_focus()` below never
                    // becomes true — clicking a `Sense::click()` widget
                    // does not grant it egui keyboard focus by itself; see
                    // `grid_ui.rs`'s equivalent real-pane click handling
                    // for the same call.
                    response.request_focus();
                    if let Some(pos) = response.interact_pointer_pos() {
                        let local = pos - response.rect.min;
                        self.chrome_add_profile.click_at_with_focus(
                            width,
                            local.x as f64,
                            local.y as f64,
                        );
                    }
                }

                // Real keyboard-to-DOM routing, same convention
                // `grid_ui.rs` already uses for real pane content: forward
                // typed text/Backspace to whichever field
                // `click_at_with_focus` last focused.
                if response.has_focus() {
                    ui.memory_mut(|mem| {
                        mem.set_focus_lock_filter(response.id, egui::EventFilter::default());
                    });
                    for event in ui.input(|i| i.events.clone()) {
                        match event {
                            egui::Event::Text(text) => {
                                for ch in text.chars() {
                                    self.chrome_add_profile.type_key(&ch.to_string());
                                }
                            }
                            egui::Event::Key {
                                key: egui::Key::Backspace,
                                pressed: true,
                                ..
                            } => {
                                self.chrome_add_profile.type_key("Backspace");
                            }
                            _ => {}
                        }
                    }
                }

                // Real live :hover — same one-frame-latency tradeoff
                // `chrome_toolbar_ui.rs` documents for its own hover wiring.
                let hover_local = response
                    .hover_pos()
                    .map(|pos| pos - response.rect.min)
                    .unwrap_or(egui::Vec2::new(-1.0, -1.0));
                self.chrome_add_profile.update_hover(
                    width,
                    hover_local.x as f64,
                    hover_local.y as f64,
                );

                for action in self.chrome_add_profile.drain_actions() {
                    use crate::chrome_bridge::ChromeAction;
                    match action {
                        ChromeAction::CreateProfileSubmit => {
                            let name = self
                                .chrome_add_profile
                                .input_value("field-name")
                                .unwrap_or_default();
                            let start_url = self
                                .chrome_add_profile
                                .input_value("field-start-url")
                                .unwrap_or_default();
                            let email = self
                                .chrome_add_profile
                                .input_value("field-email")
                                .unwrap_or_default();
                            let password = self
                                .chrome_add_profile
                                .input_value("field-password")
                                .unwrap_or_default();
                            let proxy = self
                                .chrome_add_profile
                                .input_value("field-proxy")
                                .unwrap_or_default();
                            self.add_profile_form = Some(super::AddProfileForm {
                                name,
                                start_url,
                                email,
                                password,
                                proxy,
                            });
                            self.create_profile();
                            if self.add_profile_form.is_none() {
                                self.chrome_add_profile_was_open = false;
                            }
                        }
                        ChromeAction::CancelAddProfile => {
                            self.add_profile_form = None;
                            self.add_profile_error = None;
                            self.chrome_add_profile_was_open = false;
                        }
                        _ => {}
                    }
                }
            });
    }
}
