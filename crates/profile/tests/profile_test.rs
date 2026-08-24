use std::io::{Read, Write};

use profile::Profile;

/// Starts a tiny real HTTP/1.1 server on loopback serving `html` to
/// exactly one request, then shuts down - a real server (genuine
/// `TcpListener`, genuine HTTP response bytes over a real socket), not a
/// mock, and doesn't depend on external network content staying stable.
fn serve_html_once(html: &str) -> std::net::SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    let body = html.to_string();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            // Drain the client's request before responding - writing a
            // response without ever reading the request risks the OS
            // resetting the connection out from under the client (seen as
            // a "SendRequest" error on hyper's side) instead of a clean
            // request/response exchange.
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

/// Like [`serve_html_once`], but serves multiple exact paths (a page plus
/// whatever it links to relatively) over as many real connections as
/// arrive - needed to test relative-URL resolution, where the `<link>`
/// href and the page it came from must share one real origin.
fn serve_routes(routes: Vec<(&'static str, String)>) -> std::net::SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/").to_string();
            let body = routes.iter().find(|(route, _)| *route == path).map(|(_, body)| body.clone()).unwrap_or_default();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

/// Like [`serve_routes`], but serves raw bytes with a caller-chosen
/// `Content-Type` instead of `String`/`text/plain` — needed to serve a
/// real PNG (binary, not UTF-8 text) for an `<img>` test.
fn serve_routes_bytes(routes: Vec<(&'static str, &'static str, Vec<u8>)>) -> std::net::SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/").to_string();
            let (content_type, body) = routes
                .iter()
                .find(|(route, _, _)| *route == path)
                .map(|(_, ct, body)| (*ct, body.clone()))
                .unwrap_or(("text/plain", Vec::new()));
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend_from_slice(&body);
            let _ = stream.write_all(&response);
            let _ = stream.flush();
        }
    });
    addr
}

/// A real encoded PNG (via the `image` crate, same decoder `image_decode`
/// itself wraps) - `width` x `height`, every pixel `color` (straight
/// RGBA8). Used to serve a real `<img src>` a test can assert painted
/// pixels against, without depending on an external image file.
fn encode_png(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba(color));
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).expect("encoding a real PNG should not fail");
    bytes
}

/// Like [`serve_routes`], but each route also carries extra *response*
/// headers (name, value pairs) — needed to deliver a real
/// `Content-Security-Policy` header on the document response, which is
/// exactly what `resolve_document` extracts and `Page::load` enforces.
type RouteWithHeaders = (&'static str, &'static [(&'static str, &'static str)], String);

fn serve_routes_with_headers(routes: Vec<RouteWithHeaders>) -> std::net::SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/").to_string();
            let (extra_headers, body) = routes
                .iter()
                .find(|(route, _, _)| *route == path)
                .map(|(_, headers, body)| (*headers, body.clone()))
                .unwrap_or((&[], String::new()));
            let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
            for (name, value) in extra_headers {
                response.push_str(&format!("{name}: {value}\r\n"));
            }
            response.push_str("\r\n");
            response.push_str(&body);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

fn unique_shmem_name(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("nimble-profile-test-{tag}-{nanos}")
}

fn worker_path() -> &'static str {
    env!("CARGO_BIN_EXE_profile-worker")
}

#[test]
fn spawns_a_real_process_and_responds_to_ping() {
    let name = unique_shmem_name("ping");
    let mut profile = Profile::spawn(worker_path(), &name, 64, 64).expect("spawn should succeed");

    assert!(profile.ping().unwrap(), "worker should reply PONG to PING");

    profile.quit();
}

#[test]
fn publishes_a_real_rendered_frame_on_startup() {
    let name = unique_shmem_name("frame");
    let profile = Profile::spawn(worker_path(), &name, 64, 64).expect("spawn should succeed");

    // The worker renders and publishes before entering its command loop,
    // but process startup + first GPU init isn't instant - poll briefly.
    let mut frame = None;
    for _ in 0..100 {
        if let Some(f) = profile.latest_frame() {
            frame = Some(f);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let pixels = frame.expect("worker should publish a frame within 5s");

    assert_eq!(pixels.len(), 64 * 64 * 4);
    // The demo page has a dark, non-black background (#1a1c2b) - at least
    // confirm the frame isn't just the shmem region's zeroed initial
    // state (which would read as fully transparent black).
    let non_zero = pixels.chunks_exact(4).any(|px| px != [0, 0, 0, 0]);
    assert!(non_zero, "rendered frame should not be all-zero");

    profile.quit();
}

#[test]
fn reload_publishes_a_new_generation() {
    let name = unique_shmem_name("reload");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");

    let mut gen0 = 0;
    for _ in 0..100 {
        gen0 = profile.frame_generation();
        if gen0 > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(gen0 > 0, "should have an initial published frame");

    profile.reload().unwrap();
    assert!(profile.frame_generation() > gen0);

    profile.quit();
}

#[test]
fn quit_makes_the_child_process_exit() {
    let name = unique_shmem_name("quit");
    let profile = Profile::spawn(worker_path(), &name, 16, 16).expect("spawn should succeed");
    // quit() consumes self and waits (with a timeout) for real exit -
    // if this hangs, the test framework's own timeout will catch it.
    profile.quit();
}

#[test]
fn navigate_fetches_a_real_page_and_renders_its_content() {
    let name = unique_shmem_name("navigate");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let frame_before = profile.latest_frame().unwrap();
    let result = profile.navigate("https://example.com/").expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed against a real URL: {result:?}");

    // A real page (all-white-ish background, real body text) looks
    // nothing like the dark demo page - pixels should visibly differ.
    let frame_after = profile.latest_frame().unwrap();
    assert_ne!(frame_before, frame_after);

    profile.quit();
}

#[test]
fn navigate_fetches_and_applies_a_real_import() {
    let imported_addr = serve_html_once("#box { background-color: #00ffff; width: 300px; height: 150px; }");
    let imported_url = format!("http://{imported_addr}/imported.css");
    let html = format!(r#"<div id="box">hi</div><style>@import "{imported_url}";</style>"#);
    let page_addr = serve_html_once(&html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("import-extract");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let pixels = profile.latest_frame().unwrap();
    let top_left = &pixels[0..4];
    assert_eq!(
        top_left,
        &[0, 255, 255, 255],
        "a real @import should be fetched and applied, got {top_left:?}"
    );

    profile.quit();
}

#[test]
fn navigate_applies_a_media_query_matching_the_real_viewport_width() {
    // Spawned at 300px wide: `min-width: 250px` should match, `max-width:
    // 100px` should not - proves the worker evaluates @media against its
    // *actual* frame width, not a hardcoded default.
    // Pure-channel colors only (0 or 255 per channel) - a mid-range hex
    // value round-trips through this render pipeline's sRGB texture
    // format with a visible gamma shift, which isn't what this test is
    // about; 0/255 endpoints are unaffected by that curve.
    let html = r#"<div id="box">hi</div><style>
        #box { background-color: #000000; width: 300px; height: 150px; }
        @media (min-width: 250px) { #box { background-color: #00ff00; } }
        @media (max-width: 100px) { #box { background-color: #0000ff; } }
    </style>"#;
    let addr = serve_html_once(html);
    let url = format!("http://{addr}/");

    let name = unique_shmem_name("media-query");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let pixels = profile.latest_frame().unwrap();
    let top_left = &pixels[0..4];
    assert_eq!(
        top_left,
        &[0, 255, 0, 255],
        "the min-width:250px rule should apply at a real 300px viewport and the max-width:100px rule should not, got {top_left:?}"
    );

    profile.quit();
}

#[test]
fn navigate_applies_a_real_style_block_extracted_from_the_fetched_page() {
    let html = r#"<div id="box">hi</div><style>#box { background-color: #00ff00; width: 300px; height: 150px; }</style>"#;
    let addr = serve_html_once(html);
    let url = format!("http://{addr}/");

    let name = unique_shmem_name("style-extract");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed against a real local server: {result:?}");

    let pixels = profile.latest_frame().unwrap();
    let top_left = &pixels[0..4];
    assert_eq!(
        top_left,
        &[0, 255, 0, 255],
        "the extracted <style> block's background-color should have painted the div's box green, got {top_left:?}"
    );

    profile.quit();
}

#[test]
fn navigate_fetches_a_linked_stylesheet_and_applies_it() {
    let css_addr = serve_html_once("#box { background-color: #ff00ff; width: 300px; height: 150px; }");
    let css_url = format!("http://{css_addr}/style.css");
    let html = format!(r#"<div id="box">hi</div><link rel="stylesheet" href="{css_url}">"#);
    let page_addr = serve_html_once(&html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("link-extract");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let pixels = profile.latest_frame().unwrap();
    let top_left = &pixels[0..4];
    assert_eq!(
        top_left,
        &[255, 0, 255, 255],
        "the fetched <link> stylesheet's background-color should have painted the div's box magenta, got {top_left:?}"
    );

    profile.quit();
}

#[test]
fn navigate_fetches_and_paints_a_real_img() {
    let png = encode_png(300, 150, [0, 255, 0, 255]);
    let html = r#"<img id="pic" src="pic.png">"#;
    let addr = serve_routes_bytes(vec![("/", "text/html", html.as_bytes().to_vec()), ("/pic.png", "image/png", png)]);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("img");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let pixels = profile.latest_frame().unwrap();
    let top_left = &pixels[0..4];
    assert_eq!(
        top_left,
        &[0, 255, 0, 255],
        "a real fetched/decoded <img> should paint its own pixels, got {top_left:?}"
    );

    profile.quit();
}

#[test]
fn navigate_with_an_unfetchable_img_src_still_renders_the_rest_of_the_page() {
    let html = r#"<div id="box">hi</div><img src="/does-not-exist.png">"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("img-missing");
    let mut profile = Profile::spawn(worker_path(), &name, 64, 64).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "a missing <img> src should not fail the whole navigation: {result:?}");

    profile.quit();
}

#[test]
fn scroll_by_shifts_the_painted_viewport_through_taller_content() {
    let html = r#"<style>#a { background-color: #ff0000; width: 100px; height: 150px; } #b { background-color: #0000ff; width: 100px; height: 150px; }</style><div id="a"></div><div id="b"></div>"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("scroll");
    let mut profile = Profile::spawn(worker_path(), &name, 100, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let before = profile.latest_frame().unwrap();
    assert_eq!(&before[0..4], &[255, 0, 0, 255], "unscrolled viewport should show #a (red) first, got {:?}", &before[0..4]);

    let scroll_result = profile.scroll_by(150.0).expect("protocol should not fail");
    assert!(scroll_result.is_ok(), "scroll should succeed: {scroll_result:?}");

    let after = profile.latest_frame().unwrap();
    assert_eq!(
        &after[0..4],
        &[0, 0, 255, 255],
        "scrolling down by #a's own height should bring #b (blue) to the top of the viewport, got {:?}",
        &after[0..4]
    );

    // A huge further delta should clamp at the real content height (still
    // #b, not scrolled past into empty/background space).
    let clamp_result = profile.scroll_by(10_000.0).expect("protocol should not fail");
    assert!(clamp_result.is_ok(), "scroll should succeed: {clamp_result:?}");
    let clamped = profile.latest_frame().unwrap();
    assert_eq!(
        &clamped[0..4],
        &[0, 0, 255, 255],
        "scrolling far past the content should clamp at the bottom, not reveal empty space, got {:?}",
        &clamped[0..4]
    );

    profile.quit();
}

#[test]
fn scroll_by_a_negative_delta_clamps_back_to_the_top() {
    let html = r#"<style>#a { background-color: #ff0000; width: 100px; height: 150px; } #b { background-color: #0000ff; width: 100px; height: 150px; }</style><div id="a"></div><div id="b"></div>"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("scroll-clamp-top");
    let mut profile = Profile::spawn(worker_path(), &name, 100, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let _ = profile.scroll_by(150.0).expect("protocol should not fail");
    let scroll_result = profile.scroll_by(-10_000.0).expect("protocol should not fail");
    assert!(scroll_result.is_ok(), "scroll should succeed: {scroll_result:?}");

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[255, 0, 0, 255],
        "a large negative scroll should clamp back to the real top (#a, red), not go negative, got {:?}",
        &pixels[0..4]
    );

    profile.quit();
}

#[test]
fn reload_resets_scroll_to_the_top() {
    let html = r#"<style>#a { background-color: #ff0000; width: 100px; height: 150px; } #b { background-color: #0000ff; width: 100px; height: 150px; }</style><div id="a"></div><div id="b"></div>"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("scroll-reload-reset");
    let mut profile = Profile::spawn(worker_path(), &name, 100, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");
    let _ = profile.scroll_by(150.0).expect("protocol should not fail");

    profile.reload().expect("protocol should not fail");

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[255, 0, 0, 255],
        "a fresh page (post-reload) should always start scrolled to the top, got {:?}",
        &pixels[0..4]
    );

    profile.quit();
}

#[test]
fn click_at_hit_tests_against_the_scrolled_document_not_the_pre_scroll_one() {
    let html = r#"<style>#a { background-color: #ff0000; width: 100px; height: 150px; } #b { background-color: #0000ff; width: 100px; height: 150px; }</style><div id="a"></div><div id="b"></div>"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("scroll-hit-test");
    let mut profile = Profile::spawn(worker_path(), &name, 100, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");
    let _ = profile.scroll_by(150.0).expect("protocol should not fail");

    // After scrolling by #a's own height, on-screen (0,0) is #b's own
    // top-left in document space - a real hit test must add the scroll
    // offset back in to land on #b, not (incorrectly) on whatever was at
    // document-space (0,0) before any scroll happened (#a).
    let click_result = profile.click_at(10.0, 10.0).expect("protocol should not fail");
    assert!(click_result.is_ok(), "click should land on a real id-addressable element: {click_result:?}");

    profile.quit();
}

#[test]
fn navigate_resolves_a_relative_link_href_against_the_page_url() {
    let html = r#"<div id="box">hi</div><link rel="stylesheet" href="style.css">"#;
    let css = "#box { background-color: #0000ff; width: 300px; height: 150px; }".to_string();
    let addr = serve_routes(vec![("/", html.to_string()), ("/style.css", css)]);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("relative-link");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let pixels = profile.latest_frame().unwrap();
    let top_left = &pixels[0..4];
    assert_eq!(
        top_left,
        &[0, 0, 255, 255],
        "a relative <link href> should resolve against the page's own URL and apply, got {top_left:?}"
    );

    profile.quit();
}

#[test]
fn navigate_resolves_a_root_relative_link_href() {
    // Page lives at a nested path - a root-relative href must resolve
    // against the origin (scheme+host+port), not the page's own path.
    let html = r#"<div id="box">hi</div><link rel="stylesheet" href="/assets/style.css">"#;
    let css = "#box { background-color: #ffff00; width: 300px; height: 150px; }".to_string();
    let addr = serve_routes(vec![("/nested/page.html", html.to_string()), ("/assets/style.css", css)]);
    let page_url = format!("http://{addr}/nested/page.html");

    let name = unique_shmem_name("root-relative-link");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let pixels = profile.latest_frame().unwrap();
    let top_left = &pixels[0..4];
    assert_eq!(
        top_left,
        &[255, 255, 0, 255],
        "a root-relative <link href> (/assets/style.css) should resolve against the page's origin regardless of its own path, got {top_left:?}"
    );

    profile.quit();
}

#[test]
fn navigate_to_a_bad_url_reports_an_error_and_renders_one() {
    let name = unique_shmem_name("navigate-error");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate("not a url").expect("protocol should not fail");
    assert!(result.is_err(), "navigating to garbage should report an error, not silently succeed");

    // The worker should still be alive and responsive afterward - a
    // failed navigation isn't fatal.
    assert!(profile.ping().unwrap());

    profile.quit();
}

#[test]
fn reload_retries_the_last_navigated_url() {
    let name = unique_shmem_name("navigate-reload");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    profile.navigate("https://example.com/").unwrap().unwrap();
    let gen_after_navigate = profile.frame_generation();

    profile.reload().unwrap();
    assert!(profile.frame_generation() > gen_after_navigate);

    profile.quit();
}

#[test]
fn a_beforeunload_listener_that_cancels_blocks_the_next_navigation() {
    let page_addr = serve_html_once(
        r#"<div id="marker" style="width:300px;height:150px;background-color:#123456;"></div>
        <script>window.addEventListener('beforeunload', (e) => { e.preventDefault(); });</script>"#,
    );
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("beforeunload-cancel");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    profile.navigate(&page_url).unwrap().unwrap();
    let frame_before = profile.latest_frame().unwrap();

    let result = profile
        .navigate("https://example.com/should-never-be-fetched")
        .expect("protocol should not fail");
    assert_eq!(
        result,
        Err("navigation canceled by beforeunload".to_string()),
        "a beforeunload listener that calls preventDefault() should cancel the navigation"
    );

    // The old page must still be the one rendered - navigation genuinely
    // never happened, not just "reported an error but replaced the page
    // anyway".
    let frame_after = profile.latest_frame().unwrap();
    assert_eq!(frame_before, frame_after);
    assert!(profile.ping().unwrap());

    profile.quit();
}

#[test]
fn a_beforeunload_listener_that_does_not_cancel_lets_navigation_proceed() {
    let page_addr = serve_html_once(
        "<script>window.addEventListener('beforeunload', () => {});</script>",
    );
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("beforeunload-allow");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    profile.navigate(&page_url).unwrap().unwrap();
    let result = profile.navigate("https://example.com/").expect("protocol should not fail");
    assert!(result.is_ok(), "a non-canceling beforeunload listener must not block navigation: {result:?}");

    profile.quit();
}

#[test]
fn load_event_fires_before_the_first_paint_of_a_navigated_page() {
    // `.b` (red) is only applied by the real `load` listener swapping the
    // marker's class - if `load` fires (and fires before the very first
    // render, matching `Context::dispatch_lifecycle_events`' place in
    // `Page::load`), the first-ever frame for this page already shows red,
    // not the initial blue.
    let page_addr = serve_html_once(
        r#"<div id="marker" class="a"></div><style>.a { background-color: #0000ff; width: 300px; height: 150px; } .b { background-color: #ff0000; width: 300px; height: 150px; }</style><script>window.addEventListener('load', () => { document.getElementById('marker').className = 'b'; });</script>"#,
    );
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("load-event");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    profile.navigate(&page_url).unwrap().unwrap();
    let pixels = profile.latest_frame().unwrap();
    let top_left = &pixels[0..4];
    assert_eq!(top_left, &[255, 0, 0, 255], "the load listener should have already run by the first paint, got {top_left:?}");

    profile.quit();
}

#[test]
fn demo_page_visit_counter_persists_via_real_local_storage_across_reload() {
    // DEMO_SCRIPT increments a real localStorage-backed `visits` counter
    // into #counter's text every (re)load. Waiting for the 50ms tick then
    // reloading and waiting again should visibly change the rendered
    // pixels twice - real proof `Context::with_storage`'s localStorage
    // reaches an actual profile-worker page, not just js-runtime's own
    // isolated unit tests.
    // 400x200: tall enough for the demo page's three paragraphs to
    // actually land inside the canvas (see the same reasoning in the
    // vsync loop's own tests) - too short a viewport would push #counter
    // below the painted area and make this test vacuous either way.
    let name = unique_shmem_name("local-storage-demo");
    let mut profile = Profile::spawn(worker_path(), &name, 400, 200).expect("spawn should succeed");

    let frame_a = wait_for_a_frame(&profile);
    std::thread::sleep(std::time::Duration::from_millis(200));
    let frame_b = profile.latest_frame().unwrap();
    assert_ne!(frame_a, frame_b, "the first tick should have painted a visits count");

    profile.reload().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let frame_c = profile.latest_frame().unwrap();
    assert_ne!(frame_b, frame_c, "reloading should bump the real persisted visits count and repaint a different number");

    profile.quit();
}

fn wait_for_a_frame(profile: &Profile) -> Vec<u8> {
    for _ in 0..100 {
        if let Some(f) = profile.latest_frame() {
            return f;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("worker should publish a frame within 5s");
}

#[test]
fn frame_generation_advances_on_its_own_without_any_reload() {
    let name = unique_shmem_name("vsync-gen");
    let profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let gen0 = profile.frame_generation();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let gen1 = profile.frame_generation();

    // ~60fps over 200ms should easily clear a handful of new frames -
    // this is the vsync loop's whole point: frames keep publishing without
    // any RELOAD (or any other command) being sent at all.
    assert!(gen1 > gen0 + 3, "frame generation should keep advancing on its own, got {gen0} -> {gen1}");

    profile.quit();
}

#[test]
fn set_fps_cap_actually_slows_down_the_render_loops_own_cadence() {
    let name = unique_shmem_name("fps-cap");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.set_fps_cap(5).expect("protocol should not fail against a live worker");
    assert!(result.is_ok(), "a positive fps cap should be accepted: {result:?}");

    let gen0 = profile.frame_generation();
    std::thread::sleep(std::time::Duration::from_millis(400));
    let gen1 = profile.frame_generation();

    // At 5fps, 400ms should produce roughly 2 new frames, nowhere near the
    // ~24 a real ~60fps loop would - proves SET_FPS_CAP actually reached
    // the render loop's own scheduling, not just returned success.
    let advanced = gen1 - gen0;
    assert!(advanced <= 4, "capped loop should advance only a couple frames in 400ms, got {advanced} ({gen0} -> {gen1})");

    profile.quit();
}

#[test]
fn set_fps_cap_rejects_a_non_positive_value() {
    let name = unique_shmem_name("fps-cap-invalid");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.set_fps_cap(0).expect("protocol should not fail against a live worker");
    assert!(result.is_err(), "a zero fps cap should be rejected, not silently accepted");

    profile.quit();
}

#[test]
fn pause_actually_stops_frame_generation_and_resume_restarts_it() {
    let name = unique_shmem_name("pause");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    profile.pause().expect("pause should reach a live worker");
    let gen_paused_start = profile.frame_generation();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let gen_paused_end = profile.frame_generation();
    assert_eq!(gen_paused_start, gen_paused_end, "a paused loop must not publish new frames at all, got {gen_paused_start} -> {gen_paused_end}");

    // Still responsive to PING while paused - not a hung/dead process.
    assert!(profile.ping().expect("ping should still reach a paused worker"));

    profile.resume().expect("resume should reach a live worker");
    let gen_resumed_start = profile.frame_generation();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let gen_resumed_end = profile.frame_generation();
    assert!(gen_resumed_end > gen_resumed_start, "resuming should restart real frame generation, got {gen_resumed_start} -> {gen_resumed_end}");

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
    // rewrites #counter's text - give it a few ticks' worth of real time.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let frame_b = profile.latest_frame().expect("should still have a frame");

    assert_ne!(
        frame_a, frame_b,
        "rendered pixels should change as the worker's per-frame loop pumps setTimeout/requestAnimationFrame and re-renders the mutated DOM"
    );

    profile.quit();
}

#[test]
fn dropping_without_quit_still_kills_the_process() {
    let name = unique_shmem_name("drop");
    {
        let _profile = Profile::spawn(worker_path(), &name, 16, 16).expect("spawn should succeed");
        // Dropped here without calling quit() - Drop must force-kill it
        // rather than leaking a live child process.
    }
    // No direct assertion possible without a process handle (consumed by
    // Drop) - this test's value is Drop not panicking/hanging, and it
    // documents the guarantee. Process leaks would only surface in a
    // real leak-detection harness, not this test.
}

/// A real forward proxy for tests: answers `CONNECT host:port HTTP/1.1`
/// with `200 Connection Established`, then splices raw bytes between the
/// client and `host:port` in both directions — a genuine tunnel, not a
/// stand-in that special-cases the request. Reports each `CONNECT`
/// target it handled on `seen`, so a test can prove traffic actually went
/// through this proxy rather than connecting directly.
fn spawn_connect_proxy(seen: std::sync::mpsc::Sender<String>) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("proxy should bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut client) = stream else { continue };
            let seen = seen.clone();
            std::thread::spawn(move || {
                let mut reader = std::io::BufReader::new(client.try_clone().expect("clone client stream"));
                let mut request_line = String::new();
                if std::io::BufRead::read_line(&mut reader, &mut request_line).is_err() || request_line.is_empty() {
                    return;
                }
                let target = request_line.trim_start_matches("CONNECT ").split(' ').next().unwrap_or("").to_string();
                loop {
                    let mut line = String::new();
                    match std::io::BufRead::read_line(&mut reader, &mut line) {
                        Ok(0) | Err(_) => return,
                        Ok(_) if line == "\r\n" => break,
                        Ok(_) => {}
                    }
                }
                let Ok(mut upstream) = std::net::TcpStream::connect(&target) else {
                    let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n");
                    return;
                };
                let _ = seen.send(target);
                let _ = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n");

                let mut upstream_reader = upstream.try_clone().expect("clone upstream stream");
                let mut client_writer = client.try_clone().expect("clone client stream");
                let client_to_upstream = std::thread::spawn(move || {
                    let _ = std::io::copy(&mut reader, &mut upstream);
                });
                let upstream_to_client = std::thread::spawn(move || {
                    let _ = std::io::copy(&mut upstream_reader, &mut client_writer);
                });
                let _ = client_to_upstream.join();
                let _ = upstream_to_client.join();
            });
        }
    });
    port
}

#[test]
fn navigate_routes_through_a_configured_proxy() {
    let page_addr = serve_html_once(r#"<div id="box">hi</div><style>#box { background-color: #00ff00; width: 32px; height: 32px; }</style>"#);
    let page_url = format!("http://{page_addr}/");

    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    let proxy_port = spawn_connect_proxy(seen_tx);

    let name = unique_shmem_name("proxy-navigate");
    let mut profile =
        Profile::spawn_with_proxy(worker_path(), &name, 32, 32, Some(&format!("127.0.0.1:{proxy_port}"))).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate through the proxy should succeed: {result:?}");

    let tunneled_target = seen_rx.recv_timeout(std::time::Duration::from_secs(5)).expect("proxy should have handled a CONNECT for the page fetch");
    assert_eq!(tunneled_target, page_addr.to_string(), "the worker's fetch should have tunneled through the proxy to the page's own address");

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(&pixels[0..4], &[0, 255, 0, 255], "the page fetched via the proxy should still render correctly");

    profile.quit();
}

#[test]
fn navigate_without_a_proxy_never_contacts_one() {
    // No proxy configured (plain `Profile::spawn`) - a page fetch must
    // connect directly. Spawning a "proxy" that would panic if it ever
    // received a connection would be a stronger assertion, but a channel
    // that simply never fires is enough to prove nothing routed through
    // it without coupling this test to the proxy helper's own internals.
    let page_addr = serve_html_once(r#"<div>direct</div>"#);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("no-proxy-navigate");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "direct navigate should succeed: {result:?}");

    profile.quit();
}

/// A real UDP DNS server for tests: answers every A query with `answer`
/// (RFC 1035 wire format, same hand-rolled shape `net::dns`'s own client
/// builds/parses) — a genuine query/response round trip, not a stand-in.
fn spawn_fake_dns_server(answer: std::net::Ipv4Addr) -> std::net::SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("dns server should bind");
    let addr = socket.local_addr().unwrap();
    std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        loop {
            let Ok((n, from)) = socket.recv_from(&mut buf) else { return };
            if n < 12 {
                continue;
            }
            let mut resp = Vec::new();
            resp.extend_from_slice(&buf[0..2]); // id
            resp.extend_from_slice(&[0x81, 0x80]); // QR=1, RD=1, RA=1
            resp.extend_from_slice(&[0x00, 0x01]); // QDCOUNT
            resp.extend_from_slice(&[0x00, 0x01]); // ANCOUNT
            resp.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
            resp.extend_from_slice(&buf[12..n]); // echoed question
            resp.extend_from_slice(&[0xC0, 0x0C]); // name = pointer to offset 12
            resp.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // TYPE=A, CLASS=IN
            resp.extend_from_slice(&[0x00, 0x00, 0x00, 0x3C]); // TTL
            resp.extend_from_slice(&[0x00, 0x04]); // RDLENGTH
            resp.extend_from_slice(&answer.octets());
            let _ = socket.send_to(&resp, from);
        }
    });
    addr
}

#[test]
fn navigate_routes_through_a_configured_dns_server() {
    // "custom.nimble.test" isn't a real domain - this only resolves (and
    // the navigate only succeeds) because the worker actually used the
    // fake DNS server's answer (127.0.0.1) instead of the OS resolver,
    // which would fail this hostname outright.
    let page_addr = serve_html_once(r#"<div id="box">hi</div><style>#box { background-color: #0000ff; width: 32px; height: 32px; }</style>"#);
    let dns_addr = spawn_fake_dns_server(std::net::Ipv4Addr::LOCALHOST);
    let page_url = format!("http://custom.nimble.test:{}/", page_addr.port());

    let name = unique_shmem_name("dns-navigate");
    let mut profile = Profile::spawn_with_dns(worker_path(), &name, 32, 32, Some(&dns_addr.to_string())).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "navigate via the custom DNS server should succeed: {result:?}");

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(&pixels[0..4], &[0, 0, 255, 255], "the page fetched via the custom resolver should still render correctly");

    profile.quit();
}

#[test]
fn navigate_without_a_dns_server_never_contacts_one() {
    let page_addr = serve_html_once(r#"<div>direct</div>"#);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("no-dns-navigate");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate(&page_url).expect("protocol should not fail");
    assert!(result.is_ok(), "direct navigate should succeed: {result:?}");

    profile.quit();
}

#[test]
fn click_at_a_real_coordinate_dispatches_a_real_click_and_changes_the_page() {
    // #target sits at the document's real top-left flow position (no
    // position/float exists in this engine - see mockup/rendering-engine-gaps.md -
    // so it's exactly where a first block-level child with no margin
    // always lands) and gets a real click listener that mutates its own
    // textContent, giving a real, observable pixel change on success.
    // <style>/<script> come *after* #target - this engine has no UA
    // stylesheet hiding <head>/<style> (see mockup/rendering-engine-gaps.md),
    // so raw CSS/JS source text placed before an element in the markup
    // renders as real visible text and pushes later content down; placing
    // it after keeps #target at its real, predictable (0,0) flow position
    // (matches the convention this file's own existing style-block test
    // already uses).
    let page_addr = serve_html_once(
        r##"<div id="target">click me</div>
        <style>#target { width: 100px; height: 40px; }</style>
        <script>document.getElementById("target").addEventListener("click", function(){ document.getElementById("target").textContent = "clicked!"; });</script>"##,
    );

    let name = unique_shmem_name("click-at");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&format!("http://{page_addr}/")).expect("protocol should not fail").expect("navigate should succeed");

    let frame_before = profile.latest_frame().unwrap();
    let result = profile.click_at(10.0, 10.0).expect("protocol should not fail");
    assert!(result.is_ok(), "a click on the real target element should succeed: {result:?}");

    let frame_after = profile.latest_frame().unwrap();
    assert_ne!(frame_before, frame_after, "the click listener's real textContent mutation should visibly change rendered pixels");

    profile.quit();
}

#[test]
fn loaded_page_selectors_and_bubbling_events_flow_through_profile_worker() {
    let page_addr = serve_html_once(
        r##"<div id="parent"><button id="child" class="action">before</button></div>
        <style>#child { width: 100px; height: 40px; }</style>
        <script>
          const child = document.querySelector('.action');
          const parent = document.querySelector('#parent');
          if (child === null || document.querySelectorAll('.action').length !== 1) throw new Error('selector failure');
          parent.addEventListener('click', event => {
            if (event.target === child && event.currentTarget === parent && event.type === 'click') parent.textContent = 'bubbled';
          });
          child.addEventListener('click', event => { if (event.currentTarget === child) child.textContent = 'target'; });
        </script>"##,
    );

    let name = unique_shmem_name("selector-event-worker");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&format!("http://{page_addr}/")).expect("protocol should not fail").expect("navigate should succeed");

    let frame_before = profile.latest_frame().unwrap();
    let result = profile.click_at(10.0, 10.0).expect("protocol should not fail");
    assert!(result.is_ok(), "the selected child should receive the native click: {result:?}");
    assert_ne!(frame_before, profile.latest_frame().unwrap(), "the parent bubbling listener should visibly update the loaded page");

    profile.quit();
}

#[test]
fn loaded_page_can_append_to_document_body_and_repaint_after_a_click() {
    // The button is visible at the top-left before its script runs. Its
    // listener appends a new node through `document.body`, proving that a
    // mutation made by page JS is picked up by the command-path repaint,
    // rather than only mutations of nodes that existed during page load.
    let page_addr = serve_html_once(
        r##"<button id="add">add</button>
        <style>#add { width: 100px; height: 40px; }</style>
        <script>
          document.getElementById('add').addEventListener('click', () => {
            const notice = document.createElement('p');
            notice.textContent = 'added after load';
            document.body.appendChild(notice);
          });
        </script>"##,
    );

    let name = unique_shmem_name("dynamic-body-worker");
    // This engine still lays out inline script source as text, so the
    // appended paragraph lands after that source. Keep the viewport tall
    // enough to observe it in this end-to-end rendering assertion.
    let mut profile = Profile::spawn(worker_path(), &name, 300, 700).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&format!("http://{page_addr}/")).expect("protocol should not fail").expect("navigate should succeed");

    let frame_before = profile.latest_frame().unwrap();
    let result = profile.click_at(10.0, 10.0).expect("protocol should not fail");
    assert!(result.is_ok(), "the button should receive the native click: {result:?}");
    assert_ne!(frame_before, profile.latest_frame().unwrap(), "a node appended through document.body should be included in the command-path repaint");

    profile.quit();
}

#[test]
fn click_at_a_point_with_no_element_reports_an_error() {
    // A real, deliberately tiny page - block layout means nothing here
    // extends anywhere near the bottom-right of a 300x150 frame, so that
    // corner is genuinely outside every real box, not just "probably".
    let page_addr = serve_html_once(r##"<div id="tiny">x</div>"##);

    let name = unique_shmem_name("click-at-empty");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&format!("http://{page_addr}/")).expect("protocol should not fail").expect("navigate should succeed");

    let result = profile.click_at(299.0, 149.0).expect("protocol should not fail");
    assert!(result.is_err(), "a click with no real element underneath should report an error, not silently succeed");

    profile.quit();
}

#[test]
fn get_bounding_client_rect_reflects_the_real_rendered_layout() {
    // #box starts blue (.narrow) sized 100x50 by real CSS. Its own click
    // handler measures itself via a real getBoundingClientRect() and only
    // swaps to red (.wide) if the measured width/height match what the
    // real layout engine actually computed - proving the rect Page::render
    // pushed via Context::set_layout_rects (after the *first* real render,
    // from navigate) is genuinely visible to a script running later, not
    // just to js-runtime's own isolated unit tests.
    let html = r#"<div id="box" class="narrow"></div>
        <style>.narrow { background-color: #0000ff; width: 100px; height: 50px; } .wide { background-color: #ff0000; width: 100px; height: 50px; }</style>
        <script>document.getElementById('box').addEventListener('click', () => {
            const r = document.getElementById('box').getBoundingClientRect();
            if (r.width === 100 && r.height === 50) {
                document.getElementById('box').className = 'wide';
            }
        });</script>"#;
    let page_addr = serve_html_once(html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("bounding-rect");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    profile.navigate(&page_url).unwrap().unwrap();
    let before = profile.latest_frame().unwrap();
    assert_eq!(&before[0..4], &[0, 0, 255, 255], "should start blue (.narrow)");

    let click_result = profile.click_at(10.0, 10.0).expect("protocol should not fail");
    assert!(click_result.is_ok(), "clicking the real box should succeed: {click_result:?}");

    let after = profile.latest_frame().unwrap();
    assert_eq!(
        &after[0..4],
        &[255, 0, 0, 255],
        "the click handler's real getBoundingClientRect() measurement should have matched, swapping to .wide (red)"
    );

    profile.quit();
}

#[test]
fn click_at_then_type_key_types_into_a_real_focused_input() {
    let page_addr = serve_html_once(r##"<input id="field"><style>#field { width: 150px; height: 30px; }</style>"##);

    let name = unique_shmem_name("type-key");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&format!("http://{page_addr}/")).expect("protocol should not fail").expect("navigate should succeed");

    let click_result = profile.click_at(10.0, 10.0).expect("protocol should not fail");
    assert!(click_result.is_ok(), "clicking the real <input> should succeed: {click_result:?}");

    let frame_before = profile.latest_frame().unwrap();
    let type_result = profile.type_key("h").expect("protocol should not fail");
    assert!(type_result.is_ok(), "typing into the real focused input should succeed: {type_result:?}");

    let frame_after = profile.latest_frame().unwrap();
    assert_ne!(frame_before, frame_after, "typing a real character should visibly change rendered pixels");

    profile.quit();
}

#[test]
fn type_key_with_nothing_focused_reports_an_error() {
    let name = unique_shmem_name("type-key-unfocused");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.type_key("x").expect("protocol should not fail");
    assert!(result.is_err(), "typing with nothing ever clicked/focused should report an error, not silently no-op");

    profile.quit();
}

#[test]
fn spawn_with_gpu_adapter_zero_opens_a_real_adapter_and_renders_correctly() {
    // Adapter 0 always exists if this dev machine can run any of this
    // workspace's other GPU tests at all - proves the index actually
    // reaches `render::GpuRenderer::new_with_adapter`, not just that the
    // worker started.
    let name = unique_shmem_name("gpu-adapter");
    let profile = Profile::spawn_with_gpu_adapter(worker_path(), &name, 32, 32, Some(0)).expect("spawn with a real adapter index should succeed");
    wait_for_a_frame(&profile);

    // The demo page's own real render - not a specific expected color, just
    // proof the process is alive, rendering, and didn't panic on startup.
    assert!(profile.latest_frame().is_some());

    profile.quit();
}

#[test]
fn spawn_with_an_out_of_range_gpu_adapter_index_never_publishes_a_frame() {
    // `ipc::FrameReader::new` creates the shared-memory region itself if it
    // attaches before any writer does (see that method's own doc), so
    // `spawn` succeeding here doesn't mean the worker is healthy - the
    // real proof the out-of-range index actually reached
    // `GpuRenderer::new_with_adapter` and panicked (no real machine
    // enumerates 9999 adapters) is that no frame ever gets published,
    // since the worker crashes before it ever reaches its own
    // `FrameWriter::new`/`publish` call.
    let name = unique_shmem_name("gpu-adapter-bad");
    let profile = Profile::spawn_with_gpu_adapter(worker_path(), &name, 32, 32, Some(9999)).expect("spawn itself succeeds - the region self-creates on the reader side");

    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(profile.frame_generation(), 0, "a worker that panicked opening the adapter should never publish a frame");
    assert!(profile.latest_frame().is_none());
}

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
        ("/", &[("Content-Security-Policy", "connect-src 'none'")], csp_marker_html()),
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
    let html = format!("<meta http-equiv=\"Content-Security-Policy\" content=\"connect-src 'none'\">{}", csp_marker_html());
    let page_addr = serve_routes_with_headers(vec![("/", &[], html), ("/data", &[], "hello".to_string())]);
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
    let page_addr = serve_routes_with_headers(vec![("/", &[], csp_marker_html()), ("/data", &[], "hello".to_string())]);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("csp-control");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(&pixels[0..4], &[0, 255, 0, 255], "without any delivered CSP the page's own same-origin fetchSync should succeed");

    profile.quit();
}
