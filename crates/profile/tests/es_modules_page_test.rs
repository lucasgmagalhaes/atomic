use profile::Profile;

mod common;
use common::*;

/// `ROADMAP.md` item 19: `<script type="module">` now really links and
/// executes (previously treated as a plain classic script - see
/// `page_source/scripts.rs`'s own doc for what changed). This proves the
/// whole path end to end: `Page::load` picks `Context::eval_module` for
/// a real `type="module"` script, which fetches a second real module
/// over the network, links a real `import`/`export` binding, and its
/// side effect (a DOM mutation) is observable in the page afterward -
/// not silently falling back to classic-script behavior.
#[test]
fn a_script_type_module_really_links_and_executes() {
    let addr = serve_routes(vec![
        (
            "/",
            r#"<div id="marker">before</div><script type="module" src="/main.mjs"></script>"#
                .to_string(),
        ),
        (
            "/main.mjs",
            "import { text } from './value.mjs'; \
             document.getElementById('marker').textContent = text;"
                .to_string(),
        ),
        (
            "/value.mjs",
            "export const text = 'module-ran';".to_string(),
        ),
    ]);

    let name = unique_shmem_name("es-module-script-tag");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&format!("http://{addr}/"))
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let marker = profile
        .evaluate("document.getElementById('marker').textContent")
        .expect("protocol should not fail")
        .expect("script must not throw");
    assert_eq!(
        marker, "module-ran",
        "a real type=module script must fetch its import, link the export, and run - \
         not silently fall back to classic-script behavior"
    );

    profile.quit();
}
