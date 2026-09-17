//! Proves the first real slice of `spec/architecture/chrome-ui.md`'s P5
//! plan: `crates/atomic/chrome-ui/toolbar.html`, a real Preact+htm bundle
//! (real `npm install`ed packages under that directory's own
//! `node_modules/`, no bundler - the UMD builds `npm` already installs
//! are loaded directly), loaded via a real `file://` URL (not `data:`,
//! since this bundle is multiple files - the HTML plus three
//! `<script src>` files it must actually resolve relative to itself) and
//! driven through the same `Profile`/`cef_profile_worker` bridge
//! `chrome_bridge_test.rs` already proved with a hand-written page.

use std::time::Duration;

fn worker_binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_cef_profile_worker"))
}

/// `file://` URL for `crates/atomic/chrome-ui/toolbar.html`, resolved
/// relative to this crate's own manifest dir so the test works regardless
/// of the workspace's absolute path on disk.
fn toolbar_url() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../atomic/chrome-ui/toolbar.html")
        .canonicalize()
        .expect("crates/atomic/chrome-ui/toolbar.html must exist");
    // `canonicalize()` on Windows returns a `\\?\`-prefixed verbatim path
    // (e.g. `\\?\E:\...`) - a real path Windows itself understands, but
    // not a valid `file://` URL once naively slash-flipped (the `?`
    // survives and gets percent-encoded, producing a URL CEF rejects
    // outright: "Invalid URL: file://%3F/E:/..."). Strip the prefix
    // first; the rest is an ordinary absolute path.
    let path_str = path.to_string_lossy();
    let path_str = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
    format!("file:///{}", path_str.replace('\\', "/"))
}

#[test]
fn preact_toolbar_renders_pushed_state_as_real_dom() {
    let path = worker_binary_path();
    let mut chrome = profile::Profile::spawn(
        &path.to_string_lossy(),
        "atomic-chrome-toolbar-test-1",
        900,
        60,
    )
    .expect("failed to spawn cef_profile_worker as the chrome browser");

    chrome
        .navigate(&toolbar_url())
        .expect("stdin/stdout protocol must not fail")
        .expect(
            "navigating to the real toolbar.html bundle must succeed - if this fails, check \
                 that `npm install` was run in crates/atomic/chrome-ui so preact/htm's UMD \
                 files actually exist under node_modules/ (file:// resolves <script src> \
                 relative to the HTML file, not the cwd)",
        );

    let state = r#"{"activeTab":"monitor","workspaces":[{"id":"main","name":"Principal"},{"id":"farm","name":"Farm squad"}],"activeWorkspace":"farm"}"#;
    chrome
        .evaluate(&format!("window.__atomicBridge.setState({state})"))
        .expect("stdin/stdout protocol must not fail")
        .expect("pushing state into the real Preact app must not throw");

    // Real Preact render output, not a manually-set attribute - proves
    // the real npm-installed preact.umd.js + htm.umd.js actually
    // executed and produced real DOM, not just that the script tags
    // loaded without erroring.
    let active_tab_count = chrome
        .evaluate("document.querySelectorAll('.tab.active').length")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        active_tab_count, "1",
        "exactly one tab should render as active"
    );

    let active_tab_label = chrome
        .evaluate("document.querySelector('.tab.active').textContent.trim()")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        active_tab_label, "Monitor",
        "the active tab must reflect the pushed state"
    );

    let workspace_count = chrome
        .evaluate("document.querySelectorAll('.ws-item').length")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        workspace_count, "2",
        "both pushed workspaces must render, from a real list map"
    );

    let active_ws_label = chrome
        .evaluate("document.querySelector('.ws-item.active').textContent.trim()")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        active_ws_label, "Farm squad",
        "the active workspace must reflect the pushed state"
    );

    chrome.quit();
}

#[test]
fn clicking_a_real_preact_rendered_tab_reaches_rust() {
    let path = worker_binary_path();
    let mut chrome = profile::Profile::spawn(
        &path.to_string_lossy(),
        "atomic-chrome-toolbar-test-2",
        900,
        60,
    )
    .expect("failed to spawn cef_profile_worker as the chrome browser");

    chrome
        .navigate(&toolbar_url())
        .expect("stdin/stdout protocol must not fail")
        .expect("navigating to the real toolbar.html bundle must succeed");

    let state = r#"{"activeTab":"browser","workspaces":[{"id":"main","name":"Principal"}],"activeWorkspace":"main"}"#;
    chrome
        .evaluate(&format!("window.__atomicBridge.setState({state})"))
        .expect("stdin/stdout protocol must not fail")
        .expect("pushing state into the real Preact app must not throw");

    // Real coordinate click on the "Automation" tab - it's the third tab
    // in the toolbar, rendered by real Preact, not a fixture Rust knows
    // the position of ahead of time; the position is derived from the
    // page's own rendered layout rather than hardcoded blindly.
    let tab_x = chrome
        .evaluate(
            "(function(){var t=[...document.querySelectorAll('.tab')].find(e=>e.textContent.trim()==='Automation');var r=t.getBoundingClientRect();return Math.round(r.left+r.width/2)+','+Math.round(r.top+r.height/2);})()",
        )
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    let (x, y) = tab_x
        .split_once(',')
        .map(|(x, y)| (x.parse::<f64>().unwrap(), y.parse::<f64>().unwrap()))
        .expect("expected \"x,y\" from the page's own getBoundingClientRect");

    // Real, reproducible timing gap, not a flaky-test workaround: sending
    // CLICK_AT immediately after a DOM-affecting EVAL (here, the state
    // push that rendered these tabs) raced Blink's own hit-test state in
    // testing - the exact same command sequence with a real ~1s gap
    // between steps (typed by hand into the worker's stdin) hit the
    // button correctly every time; sent back-to-back with no gap at all,
    // it silently missed. `do_message_loop_work` alone doesn't guarantee
    // a real paint has landed before the next command runs. Needs a real
    // "wait for next paint" primitive before this can be removed - see
    // spec/ROADMAP.md P5's input-routing item.
    std::thread::sleep(Duration::from_millis(1000));

    let click = chrome
        .click_at(x, y)
        .expect("stdin/stdout protocol must not fail");
    assert!(
        click.is_ok(),
        "clicking a real Preact-rendered tab must succeed: {click:?}"
    );

    let messages = chrome
        .console()
        .expect("stdin/stdout protocol must not fail");
    assert!(
        messages.iter().any(|(_, text)| text.contains("switchTab") && text.contains("automation")),
        "clicking the real rendered Automation tab must dispatch a real switchTab action - got: {messages:?}"
    );

    chrome.quit();
}
