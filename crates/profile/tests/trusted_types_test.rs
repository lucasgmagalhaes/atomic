use profile::Profile;

mod common;
use common::*;

/// Shared fixture for the Trusted Types end-to-end tests: a page script
/// tries a direct `innerHTML` injection into #marker. Under enforcement
/// that throws (painting green via .blocked); without it, assignment
/// succeeds (painting blue via .injected). #marker starts red either way,
/// so an unexpected outcome is never confusable with "script didn't run".
fn trusted_types_page_html() -> String {
    r#"<style>.idle { width: 300px; height: 150px; background-color: #ff0000; } .blocked { width: 300px; height: 150px; background-color: #00ff00; } .injected { width: 300px; height: 150px; background-color: #0000ff; }</style>
    <div id="marker" class="idle"></div>
    <script>try { document.getElementById('marker').innerHTML = '<b>payload</b>'; document.getElementById('marker').className = 'injected'; } catch (e) { document.getElementById('marker').className = 'blocked'; }</script>"#
        .to_string()
}

#[test]
fn require_trusted_types_for_delivered_by_a_meta_tag_blocks_direct_innerhtml() {
    // Same delivery channel the connect-src tests exercise (<meta
    // http-equiv>), proving the directive reaches sink enforcement through
    // the real page-load pipeline before any page script runs.
    let html = format!("<meta http-equiv=\"Content-Security-Policy\" content=\"require-trusted-types-for 'script'\">{}", trusted_types_page_html());
    let page_addr = serve_html_once(&html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("tt-meta");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    assert_eq!(
        &profile.latest_frame().unwrap()[0..4],
        &[0, 255, 0, 255],
        "the delivered require-trusted-types-for policy must have blocked the page's own direct innerHTML assignment"
    );

    profile.quit();
}

#[test]
fn without_the_directive_the_same_injection_succeeds() {
    // The control for the test above: identical page, no meta tag -
    // proving it fails because of Trusted Types enforcement, not because
    // innerHTML assignment or the try/catch marker logic is broken.
    let page_addr = serve_html_once(&trusted_types_page_html());
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("tt-control");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    assert_eq!(
        &profile.latest_frame().unwrap()[0..4],
        &[0, 0, 255, 255],
        "without a delivered policy the same direct innerHTML assignment must succeed"
    );

    profile.quit();
}

#[test]
fn a_policy_wrapped_value_still_gets_through_under_enforcement() {
    // The correct-usage path end to end: under a meta-delivered policy,
    // a page that routes its markup through trustedTypes.createPolicy
    // (...).createHTML() assigns fine and paints blue like the control.
    let html = format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"require-trusted-types-for 'script'\">{}",
        r#"<style>#marker { width: 300px; height: 150px; background-color: #ff0000; } #marker.good { background-color: #00ff00; } </style>
        <div id="marker"></div>
        <script>const p = trustedTypes.createPolicy('safe', { createHTML: (s) => s }); try { document.getElementById('marker').innerHTML = p.createHTML('<b>ok</b>'); document.getElementById('marker').className = 'good'; } catch (e) { }</script>"#
    );
    let page_addr = serve_html_once(&html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("tt-policy-ok");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    assert_eq!(
        &profile.latest_frame().unwrap()[0..4],
        &[0, 255, 0, 255],
        "a policy-created TrustedHTML value must be accepted by the enforced sink"
    );

    profile.quit();
}
