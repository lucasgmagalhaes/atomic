use profile::Profile;

mod common;
use common::*;

/// Shared fixture for the EVAL/adopted-stylesheet pixel test: #marker
/// fills the whole 300x150 frame red via the page's own `<style>` - so
/// the only way that corner can turn green afterwards is a real adopted
/// stylesheet reaching the layout cascade through an EVAL'd mutation.
fn eval_marker_html() -> String {
    r#"<div id="marker"></div>
    <style>#marker { width: 300px; height: 150px; background-color: #ff0000; }</style>"#
        .to_string()
}

#[test]
fn an_adopted_stylesheets_insert_rule_actually_changes_painted_pixels() {
    // The gap JS_ENGINE_CAPABILITY_MATRIX.md item 6 called out: the CSSOM
    // mutation path (CSSStyleSheet.insertRule + document.adoptedStyleSheets,
    // merged into the real cascade by Page::layout) had only js-runtime unit
    // coverage because this protocol had no way to trigger an arbitrary
    // mutation from a test. EVAL is that missing command, so this closes it:
    // red pixels before, green after one EVAL'd stylesheet insertion - which
    // can only happen if the mutation really flowed eval -> adopted text ->
    // cascade -> layout -> paint.
    let page_addr = serve_html_once(&eval_marker_html());
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("eval-adopted-css");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile.navigate(&page_url).unwrap().unwrap();

    assert_eq!(
        &profile.latest_frame().unwrap()[0..4],
        &[255, 0, 0, 255],
        "the page's own <style> should paint #marker red first"
    );

    let result = profile
        .evaluate(
            "var sheet = new CSSStyleSheet(); sheet.insertRule('#marker { background-color: #00ff00; }'); document.adoptedStyleSheets = [sheet]; 'adopted'",
        )
        .expect("protocol should not fail")
        .expect("the stylesheet-mutation script should run without throwing");
    assert_eq!(result, "adopted");

    // The frame read here was published by the worker's EVAL handler before
    // it replied - reaching this assertion at all means the mutation went
    // through a real relayout+repaint, not just into the JS heap.
    assert_eq!(
        &profile.latest_frame().unwrap()[0..4],
        &[0, 255, 0, 255],
        "insertRule into an adopted stylesheet must change what actually gets painted"
    );

    profile.quit();
}
