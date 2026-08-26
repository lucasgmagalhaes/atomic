//! Real end-to-end test: a genuine DPAPI-protected master key (via the
//! real Windows `CryptProtectData`, this machine's own user context - the
//! same OS primitive Chrome itself uses, round-tripped through
//! `CryptUnprotectData`) and real AES-256-GCM-encrypted cookie values, laid
//! out exactly like a real Chrome `Local State` + `Cookies` database. No
//! mocks - if this passes, the crate's decryption path genuinely works
//! against real Windows crypto.
#![cfg(windows)]

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit};
use base64::Engine;
use import::{import_cookies, recover_master_key};

fn temp_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join("nimble-import-crypto-test")
        .join(name)
}

/// Real `CryptProtectData` - the encrypt-side counterpart of
/// `crate::dpapi::unprotect`, only needed here to build a genuine
/// DPAPI-wrapped blob for the test to unwrap (this crate itself never
/// wraps, only unwraps - Chrome is always the one that wraps in reality).
fn dpapi_protect(data: &[u8]) -> Vec<u8> {
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    let mut in_blob = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &mut in_blob,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            &mut out_blob,
        )
    };
    assert_ne!(
        ok,
        0,
        "CryptProtectData should succeed on this machine: {}",
        std::io::Error::last_os_error()
    );
    let bytes =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
    unsafe { windows_sys::Win32::Foundation::LocalFree(out_blob.pbData as *mut core::ffi::c_void) };
    bytes
}

fn write_real_local_state(user_data_dir: &std::path::Path, raw_key: &[u8; 32]) {
    std::fs::create_dir_all(user_data_dir).unwrap();
    let wrapped = dpapi_protect(raw_key);
    let mut with_prefix = b"DPAPI".to_vec();
    with_prefix.extend_from_slice(&wrapped);
    let encoded = base64::engine::general_purpose::STANDARD.encode(&with_prefix);
    let local_state = format!(r#"{{"os_crypt": {{"encrypted_key": "{encoded}"}}}}"#);
    std::fs::write(user_data_dir.join("Local State"), local_state).unwrap();
}

fn encrypt_cookie_value(raw_key: &[u8; 32], plaintext: &str) -> Vec<u8> {
    let cipher = Aes256Gcm::new_from_slice(raw_key).unwrap();
    let nonce_bytes: [u8; 12] = *b"nimble-nonce"; // 12 real bytes, fixed for test determinism
    let ciphertext = cipher
        .encrypt((&nonce_bytes).into(), plaintext.as_bytes())
        .unwrap();
    let mut blob = b"v10".to_vec();
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    blob
}

fn write_real_cookies_db(path: &std::path::Path, raw_key: &[u8; 32]) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "CREATE TABLE cookies (host_key TEXT, name TEXT, encrypted_value BLOB, path TEXT, is_secure INTEGER, is_httponly INTEGER, expires_utc INTEGER)",
        [],
    )
    .unwrap();
    let encrypted = encrypt_cookie_value(raw_key, "the-real-session-value");
    conn.execute(
        "INSERT INTO cookies (host_key, name, encrypted_value, path, is_secure, is_httponly, expires_utc) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![".example.com", "session", encrypted, "/", 1i64, 1i64, 0i64],
    )
    .unwrap();
}

#[test]
fn recovers_the_real_master_key_and_decrypts_a_real_cookie_end_to_end() {
    let dir = temp_dir("full-round-trip");
    let _ = std::fs::remove_dir_all(&dir);
    let raw_key = [0x42u8; 32];

    write_real_local_state(&dir, &raw_key);
    let profile_dir = dir.join("Default");
    std::fs::create_dir_all(profile_dir.join("Network")).unwrap();
    write_real_cookies_db(&profile_dir.join("Network").join("Cookies"), &raw_key);

    let recovered_key = recover_master_key(&dir)
        .expect("a real Local State with a genuinely DPAPI-wrapped key should recover");
    assert_eq!(
        recovered_key, raw_key,
        "the real CryptUnprotectData round trip must recover the exact original key"
    );

    let cookies = import_cookies(&profile_dir.join("Network").join("Cookies"), &recovered_key)
        .expect("cookies should decrypt with the real recovered key");
    assert_eq!(cookies.len(), 1);
    assert_eq!(cookies[0].host, ".example.com");
    assert_eq!(cookies[0].name, "session");
    assert_eq!(
        cookies[0].value, "the-real-session-value",
        "AES-256-GCM decryption should recover the exact real plaintext"
    );
    assert!(cookies[0].is_secure);
    assert!(cookies[0].is_http_only);
    assert_eq!(
        cookies[0].expires, None,
        "expires_utc of 0 is Chrome's own session-cookie sentinel"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cookie_encrypted_under_a_different_key_is_skipped_not_returned_as_garbage() {
    let dir = temp_dir("wrong-key");
    let _ = std::fs::remove_dir_all(&dir);
    let real_key = [0x11u8; 32];
    let wrong_key = [0x22u8; 32];

    let profile_dir = dir.join("Default");
    std::fs::create_dir_all(profile_dir.join("Network")).unwrap();
    write_real_cookies_db(&profile_dir.join("Network").join("Cookies"), &real_key);

    let cookies = import_cookies(&profile_dir.join("Network").join("Cookies"), &wrong_key)
        .expect("import itself should still succeed");
    assert!(
    cookies.is_empty(),
    "a row that fails real GCM authentication must be skipped, not returned with garbage plaintext"
  );

    let _ = std::fs::remove_dir_all(&dir);
}
