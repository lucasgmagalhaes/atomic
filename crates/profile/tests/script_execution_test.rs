use profile::Profile;

mod common;
use common::*;

#[test]
fn clicking_a_real_anchor_navigates_to_its_href() {
    let dest_addr = serve_html_once(r#"<div id="marker">arrived</div>"#);
    let dest_url = format!("http://{dest_addr}/dest");
    let start_html = format!(r#"<a id="link" href="{dest_url}">go</a>"#);
    let start_addr = serve_html_once(&start_html);
    let start_url = format!("http://{start_addr}/");

    let name = unique_shmem_name("anchor-nav");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&start_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let click_result = profile.click("#link").expect("protocol should not fail");
    assert!(
        click_result.is_ok(),
        "click should succeed: {click_result:?}"
    );

    let marker = profile
        .evaluate("document.getElementById('marker') ? document.getElementById('marker').textContent : 'missing'")
        .expect("protocol should not fail")
        .expect("script must not throw");
    assert_eq!(
        marker, "arrived",
        "clicking the anchor should have navigated to its real href"
    );

    profile.quit();
}

#[test]
fn navigate_fetches_and_runs_a_real_external_script() {
    let script_addr =
        serve_html_once("document.getElementById('box').textContent = 'ran-from-external';");
    let script_url = format!("http://{script_addr}/script.js");
    let html = format!(r#"<div id="box">hi</div><script src="{script_url}"></script>"#);
    let page_addr = serve_html_once(&html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("external-script");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let text = profile
        .evaluate("document.getElementById('box').textContent")
        .expect("protocol should not fail")
        .expect("script must not throw");
    assert_eq!(
        text, "ran-from-external",
        "the fetched external <script src> should have run and mutated the DOM"
    );

    profile.quit();
}

#[test]
fn a_deferred_external_script_runs_after_a_later_plain_script_despite_coming_first_in_markup() {
    let deferred_addr =
        serve_html_once("document.getElementById('log').textContent += 'deferred;';");
    let deferred_url = format!("http://{deferred_addr}/deferred.js");
    let html = format!(
        r#"<div id="log"></div>
        <script defer src="{deferred_url}"></script>
        <script>document.getElementById('log').textContent += 'inline;';</script>"#
    );
    let page_addr = serve_html_once(&html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("defer-script-order");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let text = profile
        .evaluate("document.getElementById('log').textContent")
        .expect("protocol should not fail")
        .expect("script must not throw");
    assert_eq!(
        text, "inline;deferred;",
        "a <script defer> must run after the later plain <script>, even though it appears \
         first in markup - defer's ordering guarantee, not document position"
    );

    profile.quit();
}

#[test]
fn js_timers_pumped_by_the_loop_visibly_change_rendered_pixels() {
    let name = unique_shmem_name("vsync-js");
    // Large enough that the counter paragraph's text actually lands inside
    // the canvas - the demo page's three paragraphs plus 20px padding
    // don't fit in a tiny viewport, and pixels outside it are never
    // touched by `composite_glyphs`, which would make this test vacuous.
    let profile = Profile::spawn(worker_path(), &name, 400, 200).expect("spawn should succeed");

    let frame_a = wait_for_a_frame(&profile);
    // The demo script's setInterval-style counter ticks every 50ms and
    // rewrites #counter's text. Wait for that visible state transition
    // rather than idling for a fixed number of ticks.
    let frame_b = wait_for_changed_frame(&profile, &frame_a);

    assert_ne!(
        frame_a, frame_b,
        "rendered pixels should change as the worker's per-frame loop pumps setTimeout/requestAnimationFrame and re-renders the mutated DOM"
    );

    profile.quit();
}

#[test]
fn evaluate_returns_the_scripts_stringified_completion_value() {
    let name = unique_shmem_name("eval-value");
    let mut profile = Profile::spawn(worker_path(), &name, 64, 64).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .evaluate("2 + 3")
        .expect("protocol should not fail against a live worker");
    assert_eq!(
        result.expect("a valid expression should not throw"),
        "5",
        "the completion value should come back stringified"
    );

    // Reads of live page state work too - not just self-contained math
    // (`nodeName`, not `tagName` - this engine only implements the former).
    let result = profile
        .evaluate("document.createElement('div').nodeName")
        .expect("protocol should not fail");
    assert_eq!(
        result.expect("a valid expression should not throw"),
        "DIV",
        "EVAL runs in the page's own context with its real DOM globals"
    );

    profile.quit();
}

#[test]
fn evaluate_reports_a_throwing_script_and_stays_alive() {
    let name = unique_shmem_name("eval-throw");
    let mut profile = Profile::spawn(worker_path(), &name, 64, 64).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .evaluate("throw new Error('boom')")
        .expect("protocol should not fail against a live worker");
    assert!(
        result.is_err(),
        "a throwing script should be reported as Err, not silently swallowed"
    );

    // The worker must survive its own page's exception and keep answering.
    assert!(
        profile.ping().expect("protocol should not fail"),
        "worker should still respond after a script threw"
    );

    // And a following EVAL still runs - one bad script doesn't poison the context.
    let result = profile
        .evaluate("40 + 2")
        .expect("protocol should not fail");
    assert_eq!(result.expect("the next script should run fine"), "42");

    profile.quit();
}
