//! Real Windows Credential Manager round-trip tests — no mocks, these
//! read/write the actual OS credential store on this machine (same
//! convention as `sandbox::windows_job`'s real-Job-Object tests).
#![cfg(windows)]

use security::keychain;

fn unique_target(label: &str) -> String {
  format!("nimble-keychain-test:{label}:{}", std::process::id())
}

#[test]
fn write_then_read_round_trips_the_exact_bytes() {
  let target = unique_target("round-trip");
  let secret = b"a 32-byte-ish secret for testing!!".to_vec();

  keychain::write_credential(&target, &secret)
    .expect("write should succeed against the real credential store");
  let read_back =
    keychain::read_credential(&target).expect("read should succeed right after a write");
  assert_eq!(read_back, secret);

  keychain::delete_credential(&target).expect("cleanup delete should succeed");
}

#[test]
fn read_missing_credential_returns_not_found() {
  let target = unique_target("never-written");
  let err = keychain::read_credential(&target)
    .expect_err("reading a credential that was never written should fail");
  assert!(
    matches!(err, keychain::KeychainError::NotFound),
    "expected NotFound, got {err:?}"
  );
}

#[test]
fn overwriting_an_existing_credential_replaces_its_value() {
  let target = unique_target("overwrite");
  keychain::write_credential(&target, b"first").expect("first write should succeed");
  keychain::write_credential(&target, b"second").expect("second write should succeed");

  let read_back = keychain::read_credential(&target).expect("read should succeed");
  assert_eq!(read_back, b"second");

  keychain::delete_credential(&target).expect("cleanup delete should succeed");
}

#[test]
fn delete_then_read_returns_not_found() {
  let target = unique_target("delete");
  keychain::write_credential(&target, b"to be deleted").expect("write should succeed");
  keychain::delete_credential(&target).expect("delete should succeed");

  let err = keychain::read_credential(&target).expect_err("reading after delete should fail");
  assert!(matches!(err, keychain::KeychainError::NotFound));
}

#[test]
fn deleting_a_missing_credential_is_not_an_error() {
  let target = unique_target("delete-missing");
  keychain::delete_credential(&target)
    .expect("deleting a never-written credential should be a no-op, not an error");
}

#[test]
fn persists_across_separate_reads_not_just_within_one_process_call() {
  let target = unique_target("persist");
  keychain::write_credential(&target, b"persisted value").expect("write should succeed");

  // Two independent reads, proving this isn't cached in-process state.
  let first = keychain::read_credential(&target).expect("first read should succeed");
  let second = keychain::read_credential(&target).expect("second read should succeed");
  assert_eq!(first, second);
  assert_eq!(first, b"persisted value");

  keychain::delete_credential(&target).expect("cleanup delete should succeed");
}
