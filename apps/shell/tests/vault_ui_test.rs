//! Real encrypted on-disk vault, no mocks - same `security::CredentialVault`
//! that crate's own tests exercise, just opened through this crate's
//! `default_vault_dir`/`open` convention.
use shell::vault_ui;

fn temp_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join("nimble-shell-vault-test").join(name)
}

#[test]
fn open_creates_an_empty_vault_and_set_persists_across_a_reopen() {
    let dir = temp_dir("persists");
    let _ = std::fs::remove_dir_all(&dir);

    {
        let mut vault = vault_ui::open(&dir).expect("should create a fresh vault");
        assert_eq!(vault.keys().count(), 0);
        vault.set("login.email", "user@example.com").expect("set should succeed");
    }

    let vault = vault_ui::open(&dir).expect("should reopen the same vault");
    assert_eq!(vault.get("login.email"), Some("user@example.com"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn default_vault_dir_is_under_the_os_temp_dir() {
    assert!(vault_ui::default_vault_dir().starts_with(std::env::temp_dir()));
}
