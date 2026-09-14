use profile::Profile;

mod common;
use common::*;

/// Shared page fixture for the CSP delivery tests: #marker fills the
/// whole 300x150 frame and a page script fetchSync's a same-origin
/// `/data` route (same-origin, so CORS/mixed-content can't be what
/// blocks it) before swapping the marker green (`ok`) or red
/// (`blocked`). `fetchSync` is synchronous, so by the first paint - the
/// only one this test reads - the swap has already happened, exactly
/// like the load-event test above relies on.
fn csp_marker_html() -> String {
    r#"<div id="marker" class="idle"></div>
    <style>.idle { width: 300px; height: 150px; } .ok { background-color: #00ff00; width: 300px; height: 150px; } .blocked { background-color: #ff0000; width: 300px; height: 150px; }</style>
    <script>const r = fetchSync(location.origin + '/data'); document.getElementById('marker').className = r.ok ? 'ok' : 'blocked';</script>"#
        .to_string()
}

#[test]
fn a_content_security_policy_response_header_on_the_document_blocks_the_pages_own_fetch() {
    let page_addr = serve_routes_with_headers(vec![
        (
            "/",
            &[("Content-Security-Policy", "connect-src 'none'")],
            csp_marker_html(),
        ),
        ("/data", &[], "hello".to_string()),
    ]);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("csp-header");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[255, 0, 0, 255],
        "the document's Content-Security-Policy response header should have blocked the page's own same-origin fetchSync"
    );

    profile.quit();
}

#[test]
fn a_meta_http_equiv_content_security_policy_tag_blocks_the_pages_own_fetch() {
    // Same enforcement, delivered the other real way: a <meta> tag in the
    // parsed HTML instead of a response header.
    let html = format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"connect-src 'none'\">{}",
        csp_marker_html()
    );
    let page_addr =
        serve_routes_with_headers(vec![("/", &[], html), ("/data", &[], "hello".to_string())]);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("csp-meta");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[255, 0, 0, 255],
        "a <meta http-equiv=Content-Security-Policy> tag should have blocked the page's own same-origin fetchSync"
    );

    profile.quit();
}

#[test]
fn without_any_csp_delivery_the_same_fetch_succeeds_and_paints_green() {
    // The control for the two tests above: identical page, no CSP header
    // and no meta tag - proving those tests fail because of CSP
    // enforcement, not because fetchSync itself is broken on navigated
    // pages or the marker logic always paints red.
    let page_addr = serve_routes_with_headers(vec![
        ("/", &[], csp_marker_html()),
        ("/data", &[], "hello".to_string()),
    ]);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("csp-control");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[0, 255, 0, 255],
        "without any delivered CSP the page's own same-origin fetchSync should succeed"
    );

    profile.quit();
}

/// Fixture for the `script-src` gating tests below: `#marker` starts red
/// and an external `<script src="/script.js">` (same-origin) flips it
/// green if - and only if - that script actually got fetched and ran.
fn script_marker_html() -> String {
    r#"<div id="marker" class="blocked"></div>
    <style>.blocked { background-color: #ff0000; width: 300px; height: 150px; } .ran { background-color: #00ff00; width: 300px; height: 150px; }</style>
    <script src="/script.js"></script>"#
        .to_string()
}

const MARKER_FLIP_SCRIPT: &str = "document.getElementById('marker').className = 'ran';";

#[test]
fn a_content_security_policy_script_src_none_blocks_fetching_the_external_script() {
    let page_addr = serve_routes_with_headers(vec![
        (
            "/",
            &[("Content-Security-Policy", "script-src 'none'")],
            script_marker_html(),
        ),
        ("/script.js", &[], MARKER_FLIP_SCRIPT.to_string()),
    ]);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("csp-script-src-header");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[255, 0, 0, 255],
        "script-src 'none' should have stopped /script.js from ever being fetched, so the marker never flips"
    );

    profile.quit();
}

#[test]
fn a_meta_content_security_policy_script_src_none_blocks_fetching_the_external_script() {
    let html = format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"script-src 'none'\">{}",
        script_marker_html()
    );
    let page_addr = serve_routes_with_headers(vec![
        ("/", &[], html),
        ("/script.js", &[], MARKER_FLIP_SCRIPT.to_string()),
    ]);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("csp-script-src-meta");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[255, 0, 0, 255],
        "a <meta> script-src 'none' should have stopped /script.js from ever being fetched"
    );

    profile.quit();
}

#[test]
fn without_a_script_src_directive_the_external_script_still_loads_and_runs() {
    // The control for the two tests above - identical page, no CSP at
    // all - proving those tests fail because of real script-src
    // enforcement, not because external <script src> loading is broken.
    let page_addr = serve_routes_with_headers(vec![
        ("/", &[], script_marker_html()),
        ("/script.js", &[], MARKER_FLIP_SCRIPT.to_string()),
    ]);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("csp-script-src-control");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[0, 255, 0, 255],
        "without a script-src directive the external script should fetch and run normally"
    );

    profile.quit();
}
