use security::{decrypt, encrypt, generate_key, Error};

#[test]
fn round_trips_plaintext() {
    let key = generate_key();
    let plaintext = b"session cookies and other secrets";
    let blob = encrypt(&key, plaintext);
    let decrypted = decrypt(&key, &blob).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn ciphertext_does_not_contain_the_plaintext_verbatim() {
    let key = generate_key();
    let plaintext = b"a very identifiable secret string";
    let blob = encrypt(&key, plaintext);
    assert!(
        !blob
            .windows(plaintext.len())
            .any(|w| w == plaintext.as_slice()),
        "encrypted blob should not contain the plaintext bytes anywhere"
    );
}

#[test]
fn decrypting_with_the_wrong_key_fails() {
    let key_a = generate_key();
    let key_b = generate_key();
    let blob = encrypt(&key_a, b"secret");
    let result = decrypt(&key_b, &blob);
    assert!(matches!(result, Err(Error::DecryptionFailed)));
}

#[test]
fn tampered_ciphertext_fails_authentication() {
    let key = generate_key();
    let mut blob = encrypt(&key, b"do not modify me");
    let last = blob.len() - 1;
    blob[last] ^= 0xFF; // flip a bit in the auth tag / ciphertext tail
    assert!(matches!(decrypt(&key, &blob), Err(Error::DecryptionFailed)));
}

#[test]
fn truncated_blob_fails_cleanly_instead_of_panicking() {
    let key = generate_key();
    assert!(matches!(
        decrypt(&key, &[1, 2, 3]),
        Err(Error::DecryptionFailed)
    ));
    assert!(matches!(decrypt(&key, &[]), Err(Error::DecryptionFailed)));
}

#[test]
fn two_encryptions_of_the_same_plaintext_produce_different_blobs() {
    // Fresh random nonce every call - reusing one would break AES-GCM.
    let key = generate_key();
    let a = encrypt(&key, b"same plaintext");
    let b = encrypt(&key, b"same plaintext");
    assert_ne!(a, b);
}

#[test]
fn generated_keys_are_not_all_zero_and_differ_between_calls() {
    let a = generate_key();
    let b = generate_key();
    assert_ne!(a, [0u8; security::KEY_LEN]);
    assert_ne!(a, b);
}
