//! Proves `spec/architecture/chrome-ui.md`'s central premise: the chrome
//! UI needs **no new binary**. `cef_profile_worker` already supports every
//! primitive it needs (`NAVIGATE` to a local bundle, `EVAL` to push state,
//! `CONSOLE` to receive actions) — a "chrome browser" is just this same
//! worker, spawned under its own `shmem_name` and pointed at a local HTML
//! bundle instead of a profile's own navigated URL.
//!
//! This is a minimal hand-written page (not the real ported mockup —
//! `spec/ROADMAP.md` P5's own next step), just enough to prove the two
//! bridge directions for real:
//! - Rust → JS: `EVAL` calling `window.__atomicBridge.setState(...)`,
//!   observed by reading back a real DOM mutation it caused.
//! - JS → Rust: a real button's `onclick` logging
//!   `console.log(JSON.stringify({action: ...}))`, observed via
//!   `Profile::console()` — the exact channel `CONSOLE` already drains.

fn worker_binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_cef_profile_worker"))
}

/// A tiny chrome-shaped page: a title bound to `__atomicBridge.setState`,
/// and a button whose `onclick` reports an action over `console.log` —
/// the same two primitives `spec/architecture/chrome-ui.md` describes,
/// just inline here rather than a real ported bundle.
const CHROME_HTML: &str = r##"
<!doctype html>
<title id="title">no workspace yet</title>
<button id="switch" onclick="console.log(JSON.stringify({action:'switchWorkspace', id:'farm-squad'}))">Switch</button>
<script>
window.__atomicBridge = {
  setState(state) {
    document.getElementById('title').textContent = state.workspaceName;
  }
};
</script>
"##;

fn data_url(html: &str) -> String {
    format!("data:text/html,{}", urlencoding_lite(html))
}

/// Minimal percent-encoding sufficient for this test's own static HTML -
/// not a general-purpose encoder (see `cef_profile_worker.rs`'s own
/// `DEMO_URL` for why a real `data:` URL must be percent-encoded at all).
fn urlencoding_lite(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[test]
fn rust_can_push_state_into_the_chrome_page_via_eval() {
    let path = worker_binary_path();
    let mut chrome =
        profile::Profile::spawn(&path.to_string_lossy(), "atomic-chrome-test-1", 900, 60)
            .expect("failed to spawn cef_profile_worker as the chrome browser");

    chrome
        .navigate(&data_url(CHROME_HTML))
        .expect("stdin/stdout protocol must not fail")
        .expect("navigating to the local chrome bundle must succeed");

    let result = chrome
        .evaluate("window.__atomicBridge.setState({workspaceName: 'Farm squad'})")
        .expect("stdin/stdout protocol must not fail");
    assert!(
        result.is_ok(),
        "pushing state into the chrome page must not throw: {result:?}"
    );

    let title = chrome
        .evaluate("document.getElementById('title').textContent")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        title, "Farm squad",
        "the real DOM must reflect the state Rust pushed, not a stale value"
    );

    chrome.quit();
}

#[test]
fn chrome_page_actions_reach_rust_via_the_console_channel() {
    let path = worker_binary_path();
    let mut chrome =
        profile::Profile::spawn(&path.to_string_lossy(), "atomic-chrome-test-2", 900, 60)
            .expect("failed to spawn cef_profile_worker as the chrome browser");

    chrome
        .navigate(&data_url(CHROME_HTML))
        .expect("stdin/stdout protocol must not fail")
        .expect("navigating to the local chrome bundle must succeed");

    // A real click on the real button, not a direct `.click()` eval call -
    // proves the same coordinate input injection a real toolbar click
    // would use also drives the bridge's JS -> Rust direction correctly.
    //
    // `CLICK_AT` right after `NAVIGATE` can race Blink's own hit-test state
    // the same way it does right after a DOM-affecting `EVAL` (see
    // `chrome_toolbar_preact_test.rs`'s identical comment and
    // `spec/ROADMAP.md` P5's own tracked hazard) - `do_message_loop_work`
    // being pumped during `navigate()` isn't proof a real paint has landed.
    // A fixed sleep is a real, tracked hack, not a fix - remove once a real
    // "wait for next paint" primitive exists.
    std::thread::sleep(std::time::Duration::from_millis(1000));

    let click = chrome
        .click_at(30.0, 15.0)
        .expect("stdin/stdout protocol must not fail");
    assert!(
        click.is_ok(),
        "clicking the real button must succeed: {click:?}"
    );

    let messages = chrome
        .console()
        .expect("stdin/stdout protocol must not fail");
    assert!(
        messages
            .iter()
            .any(|(_, text)| text.contains("switchWorkspace") && text.contains("farm-squad")),
        "the button's real console.log action must reach Rust via CONSOLE - got: {messages:?}"
    );

    chrome.quit();
}
