use security::updater::{self, Manifest};

fn temp_file(tag: &str, contents: &[u8]) -> std::path::PathBuf {
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_nanos();
  let path = std::env::temp_dir().join(format!("nimble-updater-test-{tag}-{nanos}.bin"));
  std::fs::write(&path, contents).unwrap();
  path
}

#[test]
fn generated_keypairs_are_not_all_zero_and_differ_between_calls() {
  let (sk1, vk1) = updater::generate_keypair();
  let (sk2, vk2) = updater::generate_keypair();
  assert_ne!(sk1.to_bytes(), [0u8; 32]);
  assert_ne!(sk1.to_bytes(), sk2.to_bytes());
  assert_ne!(vk1.to_bytes(), vk2.to_bytes());
}

#[test]
fn hash_file_matches_a_known_sha256() {
  let path = temp_file("hash", b"hello world");
  let hash = updater::hash_file(&path).unwrap();
  // sha256("hello world")
  let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
  let actual = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
  assert_eq!(actual, expected);
}

#[test]
fn manifest_round_trips_through_bytes() {
  let manifest = Manifest {
    version: "1.2.3".to_string(),
    sha256: [7u8; 32],
  };
  let bytes = manifest.to_bytes();
  let parsed = Manifest::parse(&bytes).unwrap();
  assert_eq!(parsed, manifest);
}

#[test]
fn full_update_flow_verifies_and_swaps_the_file() {
  let (signing_key, verifying_key) = updater::generate_keypair();
  let artifact = temp_file("artifact", b"new browser binary bytes");
  let install = std::env::temp_dir().join(format!(
    "nimble-updater-test-install-{}.bin",
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap()
      .as_nanos()
  ));
  std::fs::write(&install, b"old browser binary bytes").unwrap();

  let (manifest, signature) = updater::sign_artifact(&signing_key, "2.0.0", &artifact).unwrap();

  updater::apply_update(&verifying_key, &manifest, &signature, &artifact, &install).unwrap();

  assert_eq!(
    std::fs::read(&install).unwrap(),
    b"new browser binary bytes"
  );
  assert!(
    !artifact.exists(),
    "rename should have moved the artifact, not copied it"
  );
}

#[test]
fn apply_update_rejects_a_signature_from_the_wrong_key() {
  let (signing_key, _real_verifying_key) = updater::generate_keypair();
  let (_other_signing_key, wrong_verifying_key) = updater::generate_keypair();

  let artifact = temp_file("wrong-key-artifact", b"payload");
  let install = temp_file("wrong-key-install", b"old");

  let (manifest, signature) = updater::sign_artifact(&signing_key, "1.0.0", &artifact).unwrap();

  let result = updater::apply_update(
    &wrong_verifying_key,
    &manifest,
    &signature,
    &artifact,
    &install,
  );
  assert!(matches!(result, Err(updater::Error::BadSignature)));
  assert_eq!(
    std::fs::read(&install).unwrap(),
    b"old",
    "install path should be untouched on a failed verify"
  );
}

#[test]
fn apply_update_rejects_a_tampered_manifest() {
  let (signing_key, verifying_key) = updater::generate_keypair();
  let artifact = temp_file("tamper-artifact", b"payload");
  let install = temp_file("tamper-install", b"old");

  let (mut manifest, signature) = updater::sign_artifact(&signing_key, "1.0.0", &artifact).unwrap();
  manifest.version = "9.9.9".to_string();

  let result = updater::apply_update(&verifying_key, &manifest, &signature, &artifact, &install);
  assert!(matches!(result, Err(updater::Error::BadSignature)));
}

#[test]
fn apply_update_rejects_an_artifact_that_does_not_match_the_signed_hash() {
  let (signing_key, verifying_key) = updater::generate_keypair();
  let artifact = temp_file("swap-artifact", b"the real payload");
  let install = temp_file("swap-install", b"old");

  let (manifest, signature) = updater::sign_artifact(&signing_key, "1.0.0", &artifact).unwrap();

  // Swap the artifact's contents after it was hashed and signed - like a
  // compromised or flaky CDN serving different bytes than what was signed.
  std::fs::write(&artifact, b"a completely different payload").unwrap();

  let result = updater::apply_update(&verifying_key, &manifest, &signature, &artifact, &install);
  assert!(matches!(result, Err(updater::Error::HashMismatch)));
  assert_eq!(
    std::fs::read(&install).unwrap(),
    b"old",
    "install path should be untouched on a hash mismatch"
  );
}

#[test]
fn parse_rejects_malformed_manifests() {
  assert!(matches!(
    Manifest::parse(b"only-one-line\n"),
    Err(updater::Error::MalformedManifest)
  ));
  assert!(matches!(
    Manifest::parse(b"1.0.0\nnothex\n"),
    Err(updater::Error::MalformedManifest)
  ));
  assert!(matches!(
    Manifest::parse(b""),
    Err(updater::Error::MalformedManifest)
  ));
}
