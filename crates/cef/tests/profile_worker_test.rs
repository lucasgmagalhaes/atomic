//! Proves `crates/profile`'s existing, unmodified `Profile` API — the same
//! code `crates/atomic` uses to drive a pane — can spawn and drive
//! `cef_profile_worker` exactly as it already drives the old
//! `profile-worker` binary. This is the actual risk `spec/ROADMAP.md` P1's
//! "switch `Profile` over to the CEF worker" item needs retired: not just
//! "does `cef_profile_worker` work when driven directly over stdin" (the
//! CDP wiring bugs already found and fixed there), but "does the *client*
//! side (`Profile::navigate`/`evaluate`/`click`, with its own retry/error
//! handling) work against it too."

fn worker_binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_cef_profile_worker"))
}

#[test]
fn profile_can_navigate_and_evaluate_against_the_cef_worker() {
    let path = worker_binary_path();
    let mut profile = profile::Profile::spawn(
        &path.to_string_lossy(),
        "atomic-cef-worker-test-1",
        800,
        600,
    )
    .expect("failed to spawn cef_profile_worker");

    let nav = profile
        .navigate("https://example.com")
        .expect("stdin/stdout protocol must not fail");
    assert_eq!(nav, Ok(()), "navigation to a real page must succeed");

    let title = profile
        .evaluate("document.title")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        title, "Example Domain",
        "must reflect the real page's real title"
    );

    profile.quit();
}

#[test]
fn profile_click_reports_a_real_error_for_a_missing_selector() {
    let path = worker_binary_path();
    let mut profile = profile::Profile::spawn(
        &path.to_string_lossy(),
        "atomic-cef-worker-test-2",
        800,
        600,
    )
    .expect("failed to spawn cef_profile_worker");

    profile
        .navigate("https://example.com")
        .expect("stdin/stdout protocol must not fail")
        .expect("navigation to a real page must succeed");

    let result = profile
        .click("#this-id-does-not-exist")
        .expect("stdin/stdout protocol must not fail");
    assert!(
        result.is_err(),
        "clicking a selector with no matching element must report a real error, not silently succeed"
    );

    profile.quit();
}

#[test]
fn profile_fill_sets_a_real_input_value_via_a_real_css_selector() {
    let path = worker_binary_path();
    let mut profile = profile::Profile::spawn(
        &path.to_string_lossy(),
        "atomic-cef-worker-test-3",
        800,
        600,
    )
    .expect("failed to spawn cef_profile_worker");

    // A real Chromium page inline via a data: URL - proves this against
    // a real CSS selector (not `#id`-only, the old engine's limitation)
    // and a real element .value round trip, not textContent. The HTML
    // itself must be percent-encoded - an unescaped `data:` URL (spaces,
    // `<`, `'`) is not a valid URL and silently loads blank instead of
    // throwing, which is exactly the failure this comment is here to
    // prevent someone "fixing" back to the unescaped form.
    let html = "data:text/html,%3Cinput%20class%3D%27target%27%20value%3D%27before%27%3E";
    profile
        .navigate(html)
        .expect("stdin/stdout protocol must not fail")
        .expect("navigating to a data: URL must succeed");

    let fill = profile
        .fill(".target", "after")
        .expect("stdin/stdout protocol must not fail");
    assert_eq!(fill, Ok(()), "fill via a real class selector must succeed");

    let value = profile
        .evaluate("document.querySelector('.target').value")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        value, "after",
        "must be a real .value assignment, not textContent"
    );

    profile.quit();
}

#[test]
fn profile_click_at_and_type_key_drive_a_real_focused_input() {
    let path = worker_binary_path();
    let mut profile = profile::Profile::spawn(
        &path.to_string_lossy(),
        "atomic-cef-worker-test-4",
        400,
        300,
    )
    .expect("failed to spawn cef_profile_worker");

    // A real autofocused input with a real `input` listener - proves
    // click_at/type_key reach CEF's actual input pipeline
    // (CefBrowserHost::send_mouse_click_event/send_key_event), not a
    // simulated DOM mutation, and that Profile's existing client API
    // (unchanged) drives it correctly against the CEF-backed worker.
    let html = "data:text/html,%3Cinput%20id%3D%22box%22%20autofocus%3E%3Cdiv%20id%3D%22out%22%3E%3C%2Fdiv%3E%3Cscript%3Edocument.getElementById('box').addEventListener('input',e%3D%3E%7Bdocument.getElementById('out').textContent%3De.target.value%3B%7D)%3B%3C%2Fscript%3E";
    profile
        .navigate(html)
        .expect("stdin/stdout protocol must not fail")
        .expect("navigating to a data: URL must succeed");

    profile
        .click_at(50.0, 20.0)
        .expect("stdin/stdout protocol must not fail")
        .expect("click_at must succeed");
    profile
        .type_key("a")
        .expect("stdin/stdout protocol must not fail")
        .expect("type_key must succeed");
    profile
        .type_key("b")
        .expect("stdin/stdout protocol must not fail")
        .expect("type_key must succeed");
    profile
        .type_key("c")
        .expect("stdin/stdout protocol must not fail")
        .expect("type_key must succeed");

    let value = profile
        .evaluate("document.getElementById('out').textContent")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        value, "abc",
        "real click_at + real type_key presses must reach the real focused input"
    );

    profile.quit();
}
