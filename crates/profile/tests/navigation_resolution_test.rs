use profile::Profile;

mod common;
use common::*;

#[test]
fn navigate_resolves_a_relative_link_href_against_the_page_url() {
    let html = r#"<div id="box">hi</div><link rel="stylesheet" href="style.css">"#;
    let css = "#box { background-color: #0000ff; width: 300px; height: 150px; }".to_string();
    let addr = serve_routes(vec![("/", html.to_string()), ("/style.css", css)]);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("relative-link");
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
    let addr = serve_routes(vec![
        ("/nested/page.html", html.to_string()),
        ("/assets/style.css", css),
    ]);
    let page_url = format!("http://{addr}/nested/page.html");

    let name = unique_shmem_name("root-relative-link");
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

    let result = profile
        .navigate("not a url")
        .expect("protocol should not fail");
    assert!(
        result.is_err(),
        "navigating to garbage should report an error, not silently succeed"
    );

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
    let page_addr =
        serve_html_once("<script>window.addEventListener('beforeunload', () => {});</script>");
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("beforeunload-allow");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    profile.navigate(&page_url).unwrap().unwrap();
    let result = profile
        .navigate("https://example.com/")
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "a non-canceling beforeunload listener must not block navigation: {result:?}"
    );

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
    assert_eq!(
        top_left,
        &[255, 0, 0, 255],
        "the load listener should have already run by the first paint, got {top_left:?}"
    );

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
    let frame_b = wait_for_changed_frame(&profile, &frame_a);
    assert_ne!(
        frame_a, frame_b,
        "the first tick should have painted a visits count"
    );

    profile.reload().unwrap();
    let frame_c = wait_for_changed_frame(&profile, &frame_b);
    assert_ne!(
        frame_b, frame_c,
        "reloading should bump the real persisted visits count and repaint a different number"
    );

    profile.quit();
}
