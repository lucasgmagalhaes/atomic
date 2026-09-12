use profile::Profile;

mod common;
use common::*;

#[test]
fn a_pages_console_output_is_drainable_over_the_protocol() {
    // Load-time script logs (one of each level) plus one EVAL'd after
    // load - CONSOLE must return all of them, leveled and flattened.
    let html = r#"<script>console.log('boot'); console.warn('careful', 7); console.error('bad');</script>"#;
    let page_addr = serve_html_once(&html.to_string());
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("console-drain");
    let mut profile = Profile::spawn(worker_path(), &name, 64, 64).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    let messages = profile
        .console()
        .expect("protocol should not fail against a live worker");
    let rendered: Vec<String> = messages.iter().map(|(l, t)| format!("{l}:{t}")).collect();
    assert_eq!(
        rendered,
        ["log:boot", "warn:careful 7", "error:bad"],
        "load-time console output must reach the host, levels intact, args space-joined"
    );

    // Draining: only post-drain messages come back next time.
    profile.evaluate("console.info('after')").unwrap().unwrap();
    let messages = profile.console().expect("protocol should not fail");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0], ("info".to_string(), "after".to_string()));

    profile.quit();
}

#[test]
fn an_uncaught_page_script_error_shows_up_in_the_console_stream() {
    // A page whose own script throws at load time: the EvalError the load
    // path swallows is still reported to the console buffer like a real
    // browser's uncaught-error printout.
    let html = r#"<div id="marker" style="width:300px;height:150px;background-color:#ff0000"></div><script>noSuchFunction();</script>"#;
    let page_addr = serve_html_once(&html.to_string());
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("console-uncaught");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    let messages = profile.console().expect("protocol should not fail");
    assert_eq!(
        messages.len(),
        1,
        "exactly one entry for the single uncaught error"
    );
    assert_eq!(messages[0].0, "error");
    assert!(
        messages[0].1.starts_with("Uncaught "),
        "got: {}",
        messages[0].1
    );

    // The worker survives its page's exception and keeps answering.
    assert!(profile.ping().expect("protocol should not fail"));

    profile.quit();
}

#[test]
fn console_is_empty_for_a_page_that_never_logs() {
    let name = unique_shmem_name("console-empty");
    let mut profile = Profile::spawn(worker_path(), &name, 64, 64).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    assert!(
        profile
            .console()
            .expect("protocol should not fail")
            .is_empty(),
        "a fresh worker with a silent page has nothing to report"
    );

    profile.quit();
}
