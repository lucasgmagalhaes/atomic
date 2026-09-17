//! Proves the full ported app (`crates/atomic/chrome-ui/index.html`,
//! `spec/ROADMAP.md` P5's mockup port) renders every screen for real
//! against a real CEF-backed browser - not just the toolbar slice
//! `chrome_toolbar_preact_test.rs` already covers. Static/fake data for
//! now (see that file's own doc); this proves the render/navigation
//! pipeline, not real app-state wiring yet.

use std::time::Duration;

fn worker_binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_cef_profile_worker"))
}

fn app_url() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../atomic/chrome-ui/index.html")
        .canonicalize()
        .expect("crates/atomic/chrome-ui/index.html must exist");
    let path_str = path.to_string_lossy();
    let path_str = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
    format!("file:///{}", path_str.replace('\\', "/"))
}

fn spawn(name: &str) -> profile::Profile {
    let path = worker_binary_path();
    profile::Profile::spawn(&path.to_string_lossy(), name, 1200, 800)
        .expect("failed to spawn cef_profile_worker as the chrome browser")
}

/// `navigate()` only waits for CEF's own `on_load_end` (the document and
/// its synchronous/`defer`red scripts have run) - it's real, but real
/// Preact still needs its own render pass afterward to actually mount
/// `window.__atomicBridge` and the app's DOM, and that isn't guaranteed
/// to have happened by the time `on_load_end` fires (a real, reproduced
/// race in testing: `evaluate("window.__atomicBridge.setState(...)")`
/// threw `Cannot read properties of undefined` immediately after
/// `navigate()` returned success). Polls for the actual condition this
/// test needs instead of guessing a fixed sleep duration (the workaround
/// `chrome_toolbar_preact_test.rs`/`chrome_bridge_test.rs` use for the
/// unrelated `CLICK_AT`-after-paint hazard, tracked in `spec/ROADMAP.md`
/// P5) - a real wait for the real, observable condition, not a fixed
/// delay standing in for one.
fn wait_for_the_app_to_mount(chrome: &mut profile::Profile) {
    for _ in 0..50 {
        let ready = chrome
            .evaluate("typeof window.__atomicBridge !== 'undefined'")
            .expect("stdin/stdout protocol must not fail")
            .expect("evaluating a trivial expression must not throw");
        if ready == "true" {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("window.__atomicBridge never appeared after navigate() returned");
}

#[test]
fn every_screen_renders_real_dom_from_the_default_state() {
    let mut chrome = spawn("atomic-chrome-app-test-1");
    chrome
        .navigate(&app_url())
        .expect("stdin/stdout protocol must not fail")
        .expect("navigating to the real index.html app must succeed");
    wait_for_the_app_to_mount(&mut chrome);

    // Default view is the grid - real panes rendered from real ACC data,
    // sliced to the default grid size (2).
    let pane_count = chrome
        .evaluate("document.querySelectorAll('.pane').length")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        pane_count, "2",
        "default grid size (2) must render exactly two real panes"
    );

    let workspace_count = chrome
        .evaluate("document.querySelectorAll('.ws-icon').length")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        workspace_count, "4",
        "all four workspaces must render in the sidebar"
    );

    let account_count = chrome
        .evaluate("document.querySelectorAll('.acc-card').length")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert_eq!(
        account_count, "6",
        "all six accounts must render in the account list"
    );

    chrome.quit();
}

#[test]
fn switching_tabs_renders_each_screens_real_content() {
    let mut chrome = spawn("atomic-chrome-app-test-2");
    chrome
        .navigate(&app_url())
        .expect("stdin/stdout protocol must not fail")
        .expect("navigating to the real index.html app must succeed");
    wait_for_the_app_to_mount(&mut chrome);

    // Drive real navigation through the same setState primitive the
    // toolbar's own onClick handlers use internally (this test exercises
    // the screens' own rendering, not click routing again - that's
    // already covered by chrome_toolbar_preact_test.rs).
    let cases: [(&str, &str); 5] = [
        (
            "monitor",
            "document.querySelectorAll('.stat-cards .card').length",
        ),
        (
            "automation",
            "document.querySelectorAll('.script-item').length",
        ),
        ("downloads", "document.querySelectorAll('.dl-row').length"),
        (
            "settings",
            "document.querySelectorAll('.settings-nav-item').length",
        ),
        (
            "onboarding",
            "document.querySelector('.btn-primary') ? '1' : '0'",
        ),
    ];

    for (view, probe) in cases {
        chrome
            .evaluate(&format!(
                "window.__atomicBridge.setState({{view: '{view}'}})"
            ))
            .expect("stdin/stdout protocol must not fail")
            .expect("pushing a view change must not throw");
        std::thread::sleep(Duration::from_millis(150));
        let result = chrome
            .evaluate(probe)
            .expect("stdin/stdout protocol must not fail")
            .expect("evaluating a trivial expression must not throw");
        assert_ne!(
            result, "0",
            "view {view:?} must render real content (probe: {probe:?})"
        );
    }

    chrome.quit();
}

#[test]
fn real_data_values_reach_the_dom_on_the_downloads_screen() {
    let mut chrome = spawn("atomic-chrome-app-test-3");
    chrome
        .navigate(&app_url())
        .expect("stdin/stdout protocol must not fail")
        .expect("navigating to the real index.html app must succeed");
    wait_for_the_app_to_mount(&mut chrome);

    chrome
        .evaluate("window.__atomicBridge.setState({view: 'downloads'})")
        .expect("stdin/stdout protocol must not fail")
        .expect("pushing a view change must not throw");

    let filename = chrome
        .evaluate("document.querySelectorAll('.dl-row')[0].textContent")
        .expect("stdin/stdout protocol must not fail")
        .expect("evaluating a trivial expression must not throw");
    assert!(
        filename.contains("idlegame-client-patch-1.9.4.zip"),
        "the real DOWNLOADS data must reach the real DOM - got: {filename:?}"
    );

    chrome.quit();
}
