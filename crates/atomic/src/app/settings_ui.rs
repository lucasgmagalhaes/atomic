//! The Settings window: Performance (pane cap, FPS cap, GPU), Credentials
//! (vault), and Import from Chrome.

use atomic::chrome_import;

use super::AtomicApp;

impl AtomicApp {
    pub(super) fn draw_settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        self.ensure_vault_open();
        let mut open = self.settings_open;
        egui::Window::new("Settings").open(&mut open).show(ctx, |ui| {
            ui.heading("Performance");
            ui.add(egui::Slider::new(&mut self.performance.max_panes, 1..=6).text("Max live panes"));

            let mut capped = self.performance.fps_cap.is_some();
            let mut fps_changed = false;
            if ui.checkbox(&mut capped, "Cap frame rate").changed() {
                self.performance.fps_cap = if capped { Some(30) } else { None };
                fps_changed = capped; // unchecking leaves the last cap in place on the worker (see apply_fps_cap_to_all_panes's doc) - nothing to (re)apply
            }
            if let Some(fps) = &mut self.performance.fps_cap {
                fps_changed |= ui.add(egui::Slider::new(fps, 1..=60).text("Max FPS per pane")).changed();
            }
            if fps_changed {
                self.apply_fps_cap_to_all_panes();
            }
            if let Some(error) = &self.performance_error {
                ui.colored_label(egui::Color32::RED, error);
            }
            ui.label("Frame cap and background throttling (pausing hidden panes) both apply live to already-running panes.");

            let adapters = neutron::paint::list_adapters();
            if adapters.is_empty() {
                ui.weak("No GPU adapters enumerated on this machine.");
            } else {
                let current_label = match self.performance.gpu_adapter {
                    Some(index) => adapters.get(index).map(|a| format!("{} ({:?})", a.name, a.device_type)).unwrap_or_else(|| "Default".to_string()),
                    None => "Default".to_string(),
                };
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("GPU").selected_text(current_label).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.performance.gpu_adapter, None, "Default");
                        for (index, adapter) in adapters.iter().enumerate() {
                            ui.selectable_value(&mut self.performance.gpu_adapter, Some(index), format!("{} ({:?})", adapter.name, adapter.device_type));
                        }
                    });
                    if ui.button("Apply (respawns all panes)").clicked() {
                        self.apply_gpu_adapter_to_all_panes();
                    }
                });
            }
            ui.label("GPU selection takes effect on the next spawn, not live - applying respawns every pane (losing its current page, same as changing a proxy).");

            ui.separator();
            ui.heading("Credentials");
            ui.label("Real AES-256-GCM encrypted vault (security::CredentialVault).");
            let mut use_keychain = self.vault_use_keychain;
            let keychain_supported = cfg!(windows);
            ui.add_enabled_ui(keychain_supported, |ui| {
                if ui.checkbox(&mut use_keychain, "Use OS keychain (Windows Credential Manager) for the master key").changed() {
                    self.switch_vault_backing(use_keychain);
                }
            });
            if !keychain_supported {
                ui.weak("OS keychain backing is only implemented on Windows so far - this vault uses a plain key file here.");
            }
            ui.label("A distinct vault per mode, not a migration - switching shows that mode's own entries, not the other mode's re-encrypted.");
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

            ui.separator();
            ui.heading("Import from Chrome");
            ui.label(format!("Opt-in, per profile - imports into the currently selected pane ({}), never automatic.", self.panes[self.selected].id));
            match chrome_import::default_profile_dir() {
                Some(dir) => {
                    ui.horizontal(|ui| {
                        if ui.button("Import History").clicked() {
                            self.import_chrome_history_to_selected_pane();
                        }
                        if ui.button("Import Bookmarks").clicked() {
                            self.import_chrome_bookmarks();
                        }
                        if ui.button("Import Cookies").clicked() {
                            self.import_chrome_cookies_to_selected_pane();
                        }
                        if ui.button("Import Passwords").clicked() {
                            self.import_chrome_passwords();
                        }
                    });
                    ui.weak("Cookies/Passwords need real Chrome DPAPI decryption - Windows only.");
                    ui.weak(format!("Reading from {}", dir.display()));
                }
                None => {
                    ui.weak("Chrome profile discovery isn't implemented on this platform yet.");
                }
            }
            if let Some(result) = &self.import_result {
                match result {
                    Ok(message) => {
                        ui.colored_label(egui::Color32::LIGHT_GREEN, message);
                    }
                    Err(message) => {
                        ui.colored_label(egui::Color32::RED, message);
                    }
                }
            }
            if !self.imported_bookmarks.is_empty() {
                egui::ScrollArea::vertical().id_salt("imported_bookmarks_list").max_height(120.0).show(ui, |ui| {
                    for bookmark in &self.imported_bookmarks {
                        ui.label(format!("{} — {} ({})", bookmark.name, bookmark.url, bookmark.folder));
                    }
                });
            }
        });
        self.settings_open = open;
    }
}
