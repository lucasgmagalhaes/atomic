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
