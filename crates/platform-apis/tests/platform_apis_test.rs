#[test]
fn fill_random_fills_the_whole_buffer() {
    let mut buf = [0u8; 32];
    platform_apis::fill_random(&mut buf).unwrap();
    // Not a randomness test (that's getrandom's job) - just a sanity check
    // that we actually wrote something instead of leaving the buffer zeroed.
    assert!(buf.iter().any(|&b| b != 0));
}
