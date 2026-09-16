//! The credential vault: open/switch backing/add/remove.

use atomic::vault_ui;

use super::AtomicApp;

impl AtomicApp {
    /// Opens the real on-disk vault (`vault_ui::open`) if it isn't open
    /// yet - lazy, so a session that never opens Settings never touches
    /// the filesystem for this. A failure (permissions, corrupted vault
    /// file) is stored in `self.vault_error` and shown in the Settings
    /// window rather than silently leaving the Credentials section blank.
    pub(super) fn ensure_vault_open(&mut self) {
        if self.vault.is_some() {
            return;
        }
        let dir = vault_ui::default_vault_dir();
        let opened = if self.vault_use_keychain {
            vault_ui::open_with_keychain(&dir)
        } else {
            vault_ui::open(&dir)
        };
        match opened {
            Ok(vault) => {
                self.vault = Some(vault);
                self.vault_error = None;
            }
            Err(e) => self.vault_error = Some(e.to_string()),
        }
    }

    /// The Settings window's "Use OS keychain" checkbox: switches which
    /// vault [`ensure_vault_open`](Self::ensure_vault_open) opens (a
    /// distinct file per mode, not a migration - see
    /// `vault_ui::open_with_keychain`'s doc) and opens it immediately so
    /// the Credentials section reflects the new mode's own real entries
    /// right away instead of showing stale ones until the next frame that
    /// happens to need it.
    pub(super) fn switch_vault_backing(&mut self, use_keychain: bool) {
        self.vault_use_keychain = use_keychain;
        self.vault = None;
        self.vault_error = None;
        self.ensure_vault_open();
    }

    /// Adds `self.vault_key_text` -> `self.vault_value_text` to the real
    /// vault (encrypts + flushes to disk immediately, see
    /// `CredentialVault::set`'s own doc) and clears both fields on
    /// success.
    pub(super) fn add_credential(&mut self) {
        let key = self.vault_key_text.trim().to_string();
        if key.is_empty() {
            return;
        }
        let Some(vault) = self.vault.as_mut() else {
            return;
        };
        match vault.set(&key, &self.vault_value_text) {
            Ok(()) => {
                self.vault_key_text.clear();
                self.vault_value_text.clear();
                self.vault_error = None;
            }
            Err(e) => self.vault_error = Some(e.to_string()),
        }
    }

    pub(super) fn remove_credential(&mut self, key: &str) {
        let Some(vault) = self.vault.as_mut() else {
            return;
        };
        if let Err(e) = vault.remove(key) {
            self.vault_error = Some(e.to_string());
        }
    }
}
