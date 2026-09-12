//! Track B: the Settings window rendered by `crate::chrome_engine` instead
//! of egui — same pattern the other three chrome spikes established,
//! additive next to the real `draw_settings_window` for now. Sliders
//! become discrete stepper buttons (this engine's click model has no
//! drag/hold tracking wired) and the GPU picker follows the toolbar's
//! "buttons list, active-highlighted" idiom instead of a dropdown (this
//! engine's paint pipeline has no popup/`<select>` rendering yet).

use super::NimbleApp;

const CHROME_WIDTH: u32 = 380;
const CHROME_HEIGHT: u32 = 520;

impl NimbleApp {
    pub(super) fn draw_chrome_settings_spike(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        self.ensure_vault_open();

        let adapters = render::list_adapters();
        let adapter_labels: Vec<String> = adapters
            .iter()
            .map(|a| format!("{} ({:?})", a.name, a.device_type))
            .collect();
        let credential_keys: Vec<String> = self
            .vault
            .as_ref()
            .map(|v| v.keys().map(str::to_string).collect())
            .unwrap_or_default();
        let bookmarks: Vec<String> = self
            .imported_bookmarks
            .iter()
            .map(|b| format!("{} — {} ({})", b.name, b.url, b.folder))
            .collect();
        let import_result_text = self.import_result.as_ref().map(|r| match r {
            Ok(message) => message.clone(),
            Err(message) => message.clone(),
        });

        self.chrome_settings.sync_settings_state(
            self.performance.max_panes as u32,
            self.performance.fps_cap,
            self.vault_use_keychain,
            &adapter_labels,
            self.performance.gpu_adapter,
            &credential_keys,
            self.performance_error.as_deref(),
            self.vault_error.as_deref(),
            import_result_text.as_deref(),
            &bookmarks,
        );

        let mut open = true;
        egui::Window::new("chrome_settings_spike")
            .open(&mut open)
            .show(ctx, |ui| {
                let width = CHROME_WIDTH;
                let pixels = self
                    .chrome_settings
                    .render(&self.chrome_gpu, width, CHROME_HEIGHT);
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [width as usize, CHROME_HEIGHT as usize],
                    &pixels,
                );
                let texture = self.chrome_settings_texture.get_or_insert_with(|| {
                    ctx.load_texture(
                        "chrome-settings",
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
                    response.request_focus();
                    if let Some(pos) = response.interact_pointer_pos() {
                        let local = pos - response.rect.min;
                        self.chrome_settings.click_at_with_focus(
                            width,
                            local.x as f64,
                            local.y as f64,
                        );
                    }
                }
                if response.has_focus() {
                    ui.memory_mut(|mem| {
                        mem.set_focus_lock_filter(response.id, egui::EventFilter::default());
                    });
                    for event in ui.input(|i| i.events.clone()) {
                        match event {
                            egui::Event::Text(text) => {
                                for ch in text.chars() {
                                    self.chrome_settings.type_key(&ch.to_string());
                                }
                            }
                            egui::Event::Key {
                                key: egui::Key::Backspace,
                                pressed: true,
                                ..
                            } => {
                                self.chrome_settings.type_key("Backspace");
                            }
                            _ => {}
                        }
                    }
                }

                for action in self.chrome_settings.drain_actions() {
                    use crate::chrome_bridge::ChromeAction;
                    match action {
                        ChromeAction::StepMaxPanes(delta) => {
                            let new_value = (self.performance.max_panes as i32 + delta).clamp(1, 6);
                            self.performance.max_panes = new_value as usize;
                        }
                        ChromeAction::StepFpsCap(delta) => {
                            if let Some(fps) = &mut self.performance.fps_cap {
                                *fps = (*fps as i32 + delta).clamp(1, 60) as u32;
                                self.apply_fps_cap_to_all_panes();
                            }
                        }
                        ChromeAction::SetFpsCapEnabled(enabled) => {
                            self.performance.fps_cap = if enabled { Some(30) } else { None };
                            if enabled {
                                self.apply_fps_cap_to_all_panes();
                            }
                        }
                        ChromeAction::SetUseKeychain(use_keychain) => {
                            self.switch_vault_backing(use_keychain);
                        }
                        ChromeAction::ApplyGpuAdapter(adapter) => {
                            self.performance.gpu_adapter = adapter;
                            self.apply_gpu_adapter_to_all_panes();
                        }
                        ChromeAction::AddCredential => {
                            let key = self
                                .chrome_settings
                                .input_value("field-cred-key")
                                .unwrap_or_default();
                            let value = self
                                .chrome_settings
                                .input_value("field-cred-value")
                                .unwrap_or_default();
                            self.vault_key_text = key;
                            self.vault_value_text = value;
                            self.add_credential();
                            self.chrome_settings.set_input_values(&[
                                ("field-cred-key", ""),
                                ("field-cred-value", ""),
                            ]);
                        }
                        ChromeAction::RemoveCredential(index) => {
                            if let Some(key) = credential_keys.get(index) {
                                self.remove_credential(key);
                            }
                        }
                        ChromeAction::ImportHistory => {
                            self.import_chrome_history_to_selected_pane()
                        }
                        ChromeAction::ImportBookmarks => self.import_chrome_bookmarks(),
                        ChromeAction::ImportCookies => {
                            self.import_chrome_cookies_to_selected_pane()
                        }
                        ChromeAction::ImportPasswords => self.import_chrome_passwords(),
                        ChromeAction::CloseSettings => self.settings_open = false,
                        _ => {}
                    }
                }
            });
        if !open {
            self.settings_open = false;
        }
    }
}
