//! `CredentialVault::open_or_create_with_keychain` against the real
//! Windows Credential Manager — no mocks, same convention as
//! `keychain_test.rs`.
#![cfg(windows)]

use security::keychain;
use security::vault::CredentialVault;

fn temp_vault_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "atomic-vault-keychain-test-{label}-{}.bin",
        std::process::id()
    ))
}

fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let target = format!("atomic-vault:{}", path.display());
    let _ = keychain::delete_credential(&target);
}

#[test]
fn set_then_reopen_reads_back_the_same_entry() {
    let path = temp_vault_path("round-trip");
    cleanup(&path);

    {
        let mut vault = CredentialVault::open_or_create_with_keychain(&path)
            .expect("first open should provision a fresh keychain key");
        vault
            .set("login.email", "user@example.com")
            .expect("set should succeed");
    }

    let vault = CredentialVault::open_or_create_with_keychain(&path)
        .expect("second open should reuse the same keychain key");
    assert_eq!(vault.get("login.email"), Some("user@example.com"));

    cleanup(&path);
}

#[test]
fn second_open_reuses_the_same_keychain_key_not_a_freshly_generated_one() {
    let path = temp_vault_path("key-reuse");
    cleanup(&path);

    let target = format!("atomic-vault:{}", path.display());

    {
        let _vault = CredentialVault::open_or_create_with_keychain(&path)
            .expect("first open should provision a key");
    }
    let key_after_first_open =
        keychain::read_credential(&target).expect("a key should now exist in the credential store");

    {
        let _vault = CredentialVault::open_or_create_with_keychain(&path)
            .expect("second open should succeed");
    }
    let key_after_second_open =
        keychain::read_credential(&target).expect("the key should still be there");

    assert_eq!(
        key_after_first_open, key_after_second_open,
        "opening twice must not rotate the key"
    );

    cleanup(&path);
}

#[test]
fn vault_file_cannot_be_decrypted_without_the_matching_keychain_key() {
    let path = temp_vault_path("wrong-key");
    cleanup(&path);

    {
        let mut vault =
            CredentialVault::open_or_create_with_keychain(&path).expect("open should succeed");
        vault.set("secret", "value").expect("set should succeed");
    }

    // Simulate a different/missing key by deleting the credential without
    // touching the vault file, then reopening - open_or_create_with_keychain
    // provisions a *fresh* key on a missing credential, and that fresh key
    // cannot decrypt the old file.
    let target = format!("atomic-vault:{}", path.display());
    keychain::delete_credential(&target).expect("delete should succeed");

    let result = CredentialVault::open_or_create_with_keychain(&path);
    assert!(
        result.is_err(),
        "reopening with a freshly generated key should fail to decrypt the old vault file"
    );

    cleanup(&path);
}

#[test]
fn distinct_vault_paths_get_distinct_keychain_entries() {
    let path_a = temp_vault_path("distinct-a");
    let path_b = temp_vault_path("distinct-b");
    cleanup(&path_a);
    cleanup(&path_b);

    {
        let mut vault_a =
            CredentialVault::open_or_create_with_keychain(&path_a).expect("open a should succeed");
        vault_a.set("k", "value-a").expect("set should succeed");
    }
    {
        let mut vault_b =
            CredentialVault::open_or_create_with_keychain(&path_b).expect("open b should succeed");
        vault_b.set("k", "value-b").expect("set should succeed");
    }

    let vault_a =
        CredentialVault::open_or_create_with_keychain(&path_a).expect("reopen a should succeed");
    let vault_b =
        CredentialVault::open_or_create_with_keychain(&path_b).expect("reopen b should succeed");
    assert_eq!(vault_a.get("k"), Some("value-a"));
    assert_eq!(vault_b.get("k"), Some("value-b"));

    cleanup(&path_a);
    cleanup(&path_b);
}
