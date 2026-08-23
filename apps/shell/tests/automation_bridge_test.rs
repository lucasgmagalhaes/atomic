//! End-to-end coverage for `automation_bridge::run_script` — the same path
//! `NimbleApp`'s "Automation" panel's Run button drives (see main.rs), just
//! called directly instead of through egui: real `BrowserView`s (real
//! spawned `profile-worker` processes), a real `WorkspaceManager` naming
//! each pane, and a real script evaluated against them.
use shell::automation_bridge::run_script;
use shell::browser_view::BrowserView;
use shell::workspace::WorkspaceManager;

const PANE_ID: &str = "pane-1";

fn workspace_with_one_pane_registered() -> WorkspaceManager {
    let mut workspace = WorkspaceManager::new();
    let active = workspace.active_index();
    workspace.add_profile(active, PANE_ID.to_string());
    workspace
}

#[test]
fn pane_goto_reaches_the_real_running_profile() {
    let workspace = workspace_with_one_pane_registered();
    let mut browser = BrowserView::spawn(200, 150);
    assert!(browser.error().is_none(), "spawn should succeed against the real workspace build");

    let result = run_script(&workspace, [(PANE_ID, &mut browser)], &format!(r#"pane("{PANE_ID}").goto("https://example.com")"#));
    assert!(result.is_ok(), "goto against a real live pane should succeed: {result:?}");
}

#[test]
fn pane_goto_with_a_bad_url_surfaces_as_an_error_string() {
    let workspace = workspace_with_one_pane_registered();
    let mut browser = BrowserView::spawn(200, 150);

    let result = run_script(&workspace, [(PANE_ID, &mut browser)], &format!(r#"pane("{PANE_ID}").goto("not-a-real-url")"#));
    assert!(result.is_err(), "a bad URL should raise a JS exception surfaced as Err, not silently succeed");
}

#[test]
fn pane_name_not_in_the_active_workspace_throws_no_pane_named() {
    // Deliberately do NOT register PANE_ID - proves `run_script` only
    // grants scripts the pane names the active workspace actually lists
    // (see `automation_bridge::run_script`'s doc), not "whatever pane it
    // was handed".
    let workspace = WorkspaceManager::new();
    let mut browser = BrowserView::spawn(200, 150);

    // `js_runtime::Context::eval` doesn't hook `JS_GetException` yet (see
    // its own doc comment), so the message here is always the same
    // placeholder - what's asserted is that it threw *at all* (`pane.goto`
    // throwing "no pane named ..." is what causes that), not the message
    // text itself.
    let result = run_script(&workspace, [(PANE_ID, &mut browser)], &format!(r#"pane("{PANE_ID}").goto("https://example.com")"#));
    assert!(result.is_err(), "an unregistered pane name should throw");
}

#[test]
fn fill_and_click_reach_the_real_demo_page_through_the_bridge() {
    let workspace = workspace_with_one_pane_registered();
    let mut browser = BrowserView::spawn(200, 150);

    // BrowserView's freshly spawned profile is showing profile-worker's
    // built-in demo page, which has a real `#counter` element.
    assert!(run_script(&workspace, [(PANE_ID, &mut browser)], &format!(r##"pane("{PANE_ID}").fill("#counter", "x")"##)).is_ok());
    assert!(run_script(&workspace, [(PANE_ID, &mut browser)], &format!(r##"pane("{PANE_ID}").click("#counter")"##)).is_ok());
    assert!(run_script(&workspace, [(PANE_ID, &mut browser)], &format!(r##"pane("{PANE_ID}").fill("#does-not-exist", "x")"##)).is_err());
}

#[test]
fn a_plain_script_with_no_pane_call_still_evaluates() {
    let workspace = workspace_with_one_pane_registered();
    let mut browser = BrowserView::spawn(200, 150);

    let result = run_script(&workspace, [(PANE_ID, &mut browser)], "1 + 1");
    assert_eq!(result, Ok("2".to_string()));
}

#[test]
fn a_script_can_address_two_different_live_panes() {
    let mut workspace = WorkspaceManager::new();
    let active = workspace.active_index();
    workspace.add_profile(active, "pane-1".to_string());
    workspace.add_profile(active, "pane-2".to_string());

    let mut browser_a = BrowserView::spawn(200, 150);
    let mut browser_b = BrowserView::spawn(200, 150);

    let result = run_script(
        &workspace,
        [("pane-1", &mut browser_a), ("pane-2", &mut browser_b)],
        r##"pane("pane-1").fill("#counter", "a"); pane("pane-2").fill("#counter", "b");"##,
    );
    assert!(result.is_ok(), "a script should be able to address multiple live panes by id: {result:?}");
}
