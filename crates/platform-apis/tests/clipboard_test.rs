// Ignored by default: touches the real OS clipboard, which is shared
// global state - running this alongside other clipboard-using processes
// (or in a headless CI runner with no clipboard) is flaky by nature.
// Run explicitly with `cargo test -- --ignored` on a machine that has one.
#[test]
#[ignore]
fn round_trips_through_the_real_os_clipboard() {
    let marker = format!("atomic-clipboard-test-{}", std::process::id());
    platform_apis::clipboard_write_text(&marker).unwrap();
    let read_back = platform_apis::clipboard_read_text().unwrap();
    assert_eq!(read_back, marker);
}
