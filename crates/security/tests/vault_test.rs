use security::vault::CredentialVault;

fn temp_paths(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    (dir.join(format!("nimble-vault-test-{pid}-{name}.bin")), dir.join(format!("nimble-vault-test-{pid}-{name}.key")))
}

fn cleanup(vault_path: &std::path::Path, key_path: &std::path::Path) {
    std::fs::remove_file(vault_path).ok();
    std::fs::remove_file(key_path).ok();
}

#[test]
fn set_then_get_round_trips_a_real_value() {
    let (vault_path, key_path) = temp_paths("roundtrip");
    cleanup(&vault_path, &key_path);

    let mut vault = CredentialVault::open_or_create(&vault_path, &key_path).unwrap();
    vault.set("login.email", "user@example.com").unwrap();
    assert_eq!(vault.get("login.email"), Some("user@example.com"));

    cleanup(&vault_path, &key_path);
}

#[test]
fn entries_persist_across_separate_opens() {
    let (vault_path, key_path) = temp_paths("persist");
    cleanup(&vault_path, &key_path);

    {
        let mut vault = CredentialVault::open_or_create(&vault_path, &key_path).unwrap();
        vault.set("login.password", "hunter2").unwrap();
    }
    {
        let vault = CredentialVault::open_or_create(&vault_path, &key_path).unwrap();
        assert_eq!(vault.get("login.password"), Some("hunter2"));
    }

    cleanup(&vault_path, &key_path);
}

#[test]
fn the_vault_file_on_disk_never_contains_the_plaintext_value() {
    let (vault_path, key_path) = temp_paths("plaintext-leak");
    cleanup(&vault_path, &key_path);

    let mut vault = CredentialVault::open_or_create(&vault_path, &key_path).unwrap();
    vault.set("login.password", "correct-horse-battery-staple").unwrap();

    let raw = std::fs::read(&vault_path).unwrap();
    let raw_text = String::from_utf8_lossy(&raw);
    assert!(!raw_text.contains("correct-horse-battery-staple"));

    cleanup(&vault_path, &key_path);
}

#[test]
fn opening_with_the_wrong_key_fails_instead_of_returning_garbage() {
    let (vault_path, key_path) = temp_paths("wrong-key");
    let (_, other_key_path) = temp_paths("wrong-key-other");
    cleanup(&vault_path, &key_path);
    cleanup(&std::path::PathBuf::new(), &other_key_path);

    {
        let mut vault = CredentialVault::open_or_create(&vault_path, &key_path).unwrap();
        vault.set("k", "v").unwrap();
    }

    // A key file that exists but holds different random bytes than the
    // one the vault was actually encrypted with.
    std::fs::write(&other_key_path, security::generate_key()).unwrap();
    let result = CredentialVault::open_or_create(&vault_path, &other_key_path);
    assert!(result.is_err());

    cleanup(&vault_path, &key_path);
    std::fs::remove_file(&other_key_path).ok();
}

#[test]
fn remove_deletes_an_entry_and_it_stays_gone_after_reopening() {
    let (vault_path, key_path) = temp_paths("remove");
    cleanup(&vault_path, &key_path);

    {
        let mut vault = CredentialVault::open_or_create(&vault_path, &key_path).unwrap();
        vault.set("a", "1").unwrap();
        vault.set("b", "2").unwrap();
        vault.remove("a").unwrap();
        assert_eq!(vault.get("a"), None);
        assert_eq!(vault.get("b"), Some("2"));
    }
    {
        let vault = CredentialVault::open_or_create(&vault_path, &key_path).unwrap();
        assert_eq!(vault.get("a"), None);
        assert_eq!(vault.get("b"), Some("2"));
    }

    cleanup(&vault_path, &key_path);
}

#[test]
fn keys_lists_every_stored_entry_name() {
    let (vault_path, key_path) = temp_paths("keys");
    cleanup(&vault_path, &key_path);

    let mut vault = CredentialVault::open_or_create(&vault_path, &key_path).unwrap();
    vault.set("login.email", "a@b.com").unwrap();
    vault.set("login.password", "secret").unwrap();

    let mut keys: Vec<&str> = vault.keys().collect();
    keys.sort();
    assert_eq!(keys, vec!["login.email", "login.password"]);

    cleanup(&vault_path, &key_path);
}

#[test]
fn a_fresh_vault_starts_empty() {
    let (vault_path, key_path) = temp_paths("fresh");
    cleanup(&vault_path, &key_path);

    let vault = CredentialVault::open_or_create(&vault_path, &key_path).unwrap();
    assert_eq!(vault.get("anything"), None);
    assert_eq!(vault.keys().count(), 0);

    cleanup(&vault_path, &key_path);
}
