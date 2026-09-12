use profile::Profile;

mod common;
use common::*;

#[test]
fn navigate_fetches_a_real_page_and_renders_its_content() {
    let name = unique_shmem_name("navigate");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let frame_before = profile.latest_frame().unwrap();
    let result = profile
        .navigate("https://example.com/")
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "navigate should succeed against a real URL: {result:?}"
    );

    // A real page (all-white-ish background, real body text) looks
    // nothing like the dark demo page - pixels should visibly differ.
    let frame_after = profile.latest_frame().unwrap();
    assert_ne!(frame_before, frame_after);

    profile.quit();
}

#[test]
fn navigate_fetches_and_applies_a_real_import() {
    let imported_addr =
        serve_html_once("#box { background-color: #00ffff; width: 300px; height: 150px; }");
    let imported_url = format!("http://{imported_addr}/imported.css");
    let html = format!(r#"<div id="box">hi</div><style>@import "{imported_url}";</style>"#);
    let page_addr = serve_html_once(&html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("import-extract");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
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
    assert!(
        result.is_ok(),
        "navigate should succeed against a real local server: {result:?}"
    );

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
    let css_addr =
        serve_html_once("#box { background-color: #ff00ff; width: 300px; height: 150px; }");
    let css_url = format!("http://{css_addr}/style.css");
    let html = format!(r#"<div id="box">hi</div><link rel="stylesheet" href="{css_url}">"#);
    let page_addr = serve_html_once(&html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("link-extract");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
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
    let addr = serve_routes_bytes(vec![
        ("/", "text/html", html.as_bytes().to_vec()),
        ("/pic.png", "image/png", png),
    ]);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("img");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
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

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "a missing <img> src should not fail the whole navigation: {result:?}"
    );

    profile.quit();
}

#[test]
fn a_stylesheet_referenced_twice_in_one_page_load_is_only_fetched_once() {
    // Two <link>s pointing at the exact same href - a real page load's
    // own ResourceCache (see crate::network::ResourceCache in
    // profile-worker) should dedup this to one real GET, not two.
    let html = r#"<div id="box">hi</div>
<link rel="stylesheet" href="shared.css">
<link rel="stylesheet" href="shared.css">"#;
    let css = "#box { background-color: #00ff00; width: 300px; height: 150px; }".to_string();
    let counts = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let addr = serve_routes_counted(
        vec![("/", html.to_string()), ("/shared.css", css)],
        counts.clone(),
    );
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("dedup-shared-css");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    // The stylesheet still applied (proves the cached response was
    // actually used, not silently dropped).
    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[0, 255, 0, 255],
        "the shared stylesheet, fetched via the cache on its second reference, should still apply"
    );

    let hits = *counts.lock().unwrap().get("/shared.css").unwrap_or(&0);
    assert_eq!(
        hits, 1,
        "two <link>s sharing one href should only cause one real GET, got {hits}"
    );

    profile.quit();
}
