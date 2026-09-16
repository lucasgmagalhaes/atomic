//! The "Import from Chrome" buttons: history, bookmarks, cookies,
//! passwords.

use atomic::chrome_import;

use super::AtomicApp;

impl AtomicApp {
    /// The Settings window's "Import History" button - opt-in, explicit,
    /// scoped to whatever pane is currently selected (never automatic,
    /// never every pane at once - see `chrome_import`'s own doc on why).
    /// Real Chrome `History` SQLite rows become real entries in the
    /// selected pane's own (now-persisted) `History`.
    pub(super) fn import_chrome_history_to_selected_pane(&mut self) {
        let Some(dir) = chrome_import::default_profile_dir() else {
            self.import_result = Some(Err(
                "Chrome profile discovery isn't implemented on this platform yet".to_string(),
            ));
            return;
        };
        match chrome_import::import_history_urls(&dir) {
            Ok(urls) => {
                let count = urls.len();
                let pane = &mut self.panes[self.selected];
                for url in urls {
                    pane.history.record(url);
                }
                self.import_result = Some(Ok(format!(
                    "Imported {count} history entries into {}",
                    pane.id
                )));
            }
            Err(e) => self.import_result = Some(Err(e)),
        }
    }

    /// The Settings window's "Import Bookmarks" button - same opt-in scope
    /// as history import. No bookmarks feature exists elsewhere in this
    /// shell yet to hand the result to, so it's just kept and shown
    /// read-only (see `imported_bookmarks`'s own doc).
    pub(super) fn import_chrome_bookmarks(&mut self) {
        let Some(dir) = chrome_import::default_profile_dir() else {
            self.import_result = Some(Err(
                "Chrome profile discovery isn't implemented on this platform yet".to_string(),
            ));
            return;
        };
        match chrome_import::import_bookmarks(&dir) {
            Ok(bookmarks) => {
                self.import_result = Some(Ok(format!("Imported {} bookmarks", bookmarks.len())));
                self.imported_bookmarks = bookmarks;
            }
            Err(e) => self.import_result = Some(Err(e)),
        }
    }

    /// The Settings window's "Import Cookies" button - real DPAPI+AES-256-
    /// GCM decryption (`chrome_import::recover_master_key`/
    /// `import_cookies`), each real cookie written into the selected
    /// pane's own on-disk `CookieJar` via `chrome_import::write_cookie_into_pane`
    /// - genuinely visible to that pane's pages on their next
    /// request/`document.cookie` read, not just logged.
    pub(super) fn import_chrome_cookies_to_selected_pane(&mut self) {
        let (Some(user_data_dir), Some(profile_dir)) = (
            chrome_import::default_user_data_dir(),
            chrome_import::default_profile_dir(),
        ) else {
            self.import_result = Some(Err(
                "Chrome profile discovery isn't implemented on this platform yet".to_string(),
            ));
            return;
        };
        let master_key = match chrome_import::recover_master_key(&user_data_dir) {
            Ok(key) => key,
            Err(e) => {
                self.import_result = Some(Err(e));
                return;
            }
        };
        match chrome_import::import_cookies(&profile_dir, &master_key) {
            Ok(cookies) => {
                let pane_id = self.panes[self.selected].id.clone();
                let mut written = 0usize;
                for cookie in &cookies {
                    if chrome_import::write_cookie_into_pane(&pane_id, cookie).is_ok() {
                        written += 1;
                    }
                }
                self.import_result = Some(Ok(format!(
                    "Imported {written}/{} cookies into {pane_id}",
                    cookies.len()
                )));
            }
            Err(e) => self.import_result = Some(Err(e)),
        }
    }

    /// The Settings window's "Import Passwords" button - same real
    /// decryption as cookies, each credential stored into the real
    /// `security::CredentialVault` (opened the same way the Credentials
    /// section itself opens it) under `chrome-import:<origin>#username`/
    /// `#password` keys.
    pub(super) fn import_chrome_passwords(&mut self) {
        let (Some(user_data_dir), Some(profile_dir)) = (
            chrome_import::default_user_data_dir(),
            chrome_import::default_profile_dir(),
        ) else {
            self.import_result = Some(Err(
                "Chrome profile discovery isn't implemented on this platform yet".to_string(),
            ));
            return;
        };
        let master_key = match chrome_import::recover_master_key(&user_data_dir) {
            Ok(key) => key,
            Err(e) => {
                self.import_result = Some(Err(e));
                return;
            }
        };
        match chrome_import::import_passwords(&profile_dir, &master_key) {
            Ok(passwords) => {
                self.ensure_vault_open();
                let Some(vault) = self.vault.as_mut() else {
                    self.import_result = Some(Err(
                        "vault isn't open - see the Credentials section's own error".to_string(),
                    ));
                    return;
                };
                let mut written = 0usize;
                for password in &passwords {
                    let username_ok = vault
                        .set(
                            &format!("chrome-import:{}#username", password.origin_url),
                            &password.username,
                        )
                        .is_ok();
                    let password_ok = vault
                        .set(
                            &format!("chrome-import:{}#password", password.origin_url),
                            &password.password,
                        )
                        .is_ok();
                    if username_ok && password_ok {
                        written += 1;
                    }
                }
                self.import_result = Some(Ok(format!(
                    "Imported {written}/{} passwords into the credential vault",
                    passwords.len()
                )));
            }
            Err(e) => self.import_result = Some(Err(e)),
        }
    }
}
