//! Same real DPAPI + AES-256-GCM round trip as `cookies_test.rs`, against
//! `Login Data`'s `logins` table.
#![cfg(windows)]

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit};
use import::import_passwords;

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join("nimble-import-crypto-test")
        .join(name)
}

fn encrypt_password(raw_key: &[u8; 32], plaintext: &str) -> Vec<u8> {
    let cipher = Aes256Gcm::new_from_slice(raw_key).unwrap();
    let nonce_bytes: [u8; 12] = *b"nimble-pwd12";
    let ciphertext = cipher
        .encrypt((&nonce_bytes).into(), plaintext.as_bytes())
        .unwrap();
    let mut blob = b"v10".to_vec();
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    blob
}

#[test]
fn imports_and_decrypts_a_real_login_row() {
    let path = temp_path("Login Data-test.sqlite");
    let _ = std::fs::remove_file(&path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    let raw_key = [0x77u8; 32];
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE logins (origin_url TEXT, username_value TEXT, password_value BLOB)",
        [],
    )
    .unwrap();
    let encrypted = encrypt_password(&raw_key, "hunter2-but-real");
    conn.execute(
        "INSERT INTO logins (origin_url, username_value, password_value) VALUES (?1, ?2, ?3)",
        rusqlite::params!["https://example.com/login", "user@example.com", encrypted],
    )
    .unwrap();

    let passwords = import_passwords(&path, &raw_key).expect("import should succeed");
    assert_eq!(passwords.len(), 1);
    assert_eq!(passwords[0].origin_url, "https://example.com/login");
    assert_eq!(passwords[0].username, "user@example.com");
    assert_eq!(
        passwords[0].password, "hunter2-but-real",
        "real AES-256-GCM decryption should recover the exact plaintext"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_row_with_a_v20_prefix_is_skipped_as_an_unsupported_scheme() {
    let path = temp_path("Login Data-v20-test.sqlite");
    let _ = std::fs::remove_file(&path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE logins (origin_url TEXT, username_value TEXT, password_value BLOB)",
        [],
    )
    .unwrap();
    let mut fake_v20 = b"v20".to_vec();
    fake_v20.extend_from_slice(&[0u8; 20]);
    conn.execute(
        "INSERT INTO logins (origin_url, username_value, password_value) VALUES (?1, ?2, ?3)",
        rusqlite::params!["https://example.com/", "user", fake_v20],
    )
    .unwrap();

    let passwords =
        import_passwords(&path, &[0u8; 32]).expect("import itself should still succeed");
    assert!(
        passwords.is_empty(),
        "a real v20 (App-Bound Encryption) row should be skipped, not misread as v10/v11"
    );

    let _ = std::fs::remove_file(&path);
}
