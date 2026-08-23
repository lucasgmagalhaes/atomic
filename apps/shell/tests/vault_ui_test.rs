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

#[cfg(windows)]
#[test]
fn open_with_keychain_uses_a_distinct_file_from_the_plain_vault() {
    let dir = temp_dir("keychain-distinct");
    let _ = std::fs::remove_dir_all(&dir);

    let mut file_vault = vault_ui::open(&dir).expect("plain vault should open");
    file_vault.set("login.email", "file-backed@example.com").expect("set should succeed");

    let mut keychain_vault = vault_ui::open_with_keychain(&dir).expect("real Windows Credential Manager should back this");
    // Real proof this is a separate vault, not the same file re-read: the
    // plain vault's entry must not silently appear here.
    assert_eq!(keychain_vault.get("login.email"), None);
    keychain_vault.set("login.email", "keychain-backed@example.com").expect("set should succeed");
    assert_eq!(file_vault.get("login.email"), Some("file-backed@example.com"), "the file-backed vault must be unaffected by the keychain-backed one");

    let reopened = vault_ui::open_with_keychain(&dir).expect("reopen should reuse the real keychain-stored key");
    assert_eq!(reopened.get("login.email"), Some("keychain-backed@example.com"), "a real reopen should see the keychain-backed vault's own persisted entry");

    // Real Windows Credential Manager entry - clean it up so repeated test
    // runs don't accumulate stray credentials in the user's own store
    // (same convention as crates/security's own vault_keychain_test.rs).
    let target = format!("nimble-vault:{}", dir.join("vault-keychain.enc").display());
    let _ = security::keychain::delete_credential(&target);
    let _ = std::fs::remove_dir_all(&dir);
}
