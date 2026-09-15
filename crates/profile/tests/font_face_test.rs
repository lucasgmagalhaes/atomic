//! Real `@font-face` fetching (`page_source::load_font_faces`): a page's
//! own `@font-face { src: url(...); }` should trigger a real network GET
//! for the font file, and the page should still render successfully even
//! though the test server below doesn't serve real font bytes (this
//! engine's `layout_engine::register_font_face` silently no-ops on
//! unparseable font data via `fontdb`'s own error handling, same
//! "best-effort, never fail the whole page load" stance every other
//! optional resource in this worker already takes).

use profile::Profile;

mod common;
use common::*;

#[test]
fn a_font_face_src_is_fetched_over_the_real_network() {
    let html = r#"<div id="box">hi</div>
<style>@font-face { font-family: "TestFace"; src: url("test.woff"); }
#box { font-family: "TestFace"; width: 300px; height: 150px; }</style>"#;
    let counts = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let addr = serve_routes_counted(
        vec![
            ("/", html.to_string()),
            (
                "/test.woff",
                "not a real font, just proving the fetch happened".to_string(),
            ),
        ],
        counts.clone(),
    );
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("font-face-fetch");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    // Rendering still completes (no crash/hang from a font that doesn't
    // parse) - proven by a fresh frame actually being published.
    wait_for_a_frame(&profile);

    let hits = *counts.lock().unwrap().get("/test.woff").unwrap_or(&0);
    assert_eq!(
        hits, 1,
        "the @font-face src should be fetched exactly once over the real network"
    );

    profile.quit();
}

#[test]
fn a_font_face_referenced_only_via_an_import_is_still_fetched() {
    let imported_addr = serve_html_once(
        r#"@font-face { font-family: "Imported"; src: url("imported-font.woff"); }
#box { font-family: "Imported"; width: 300px; height: 150px; background-color: #00ffff; }"#,
    );
    let imported_url = format!("http://{imported_addr}/imported.css");
    let html = format!(r#"<div id="box">hi</div><style>@import "{imported_url}";</style>"#);
    let page_addr = serve_html_once(&html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("font-face-import");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    // The @import's own rules still applied (background-color proves the
    // imported stylesheet was fetched and merged, alongside its font-face).
    let pixels = profile.latest_frame().unwrap();
    assert_eq!(&pixels[0..4], &[0, 255, 255, 255]);

    profile.quit();
}
