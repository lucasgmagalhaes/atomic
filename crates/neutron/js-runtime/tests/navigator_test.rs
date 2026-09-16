use js_runtime::{Context, Runtime};

fn eval(script: &str) -> String {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(script, "<test>")
        .expect("navigator script should evaluate")
}

#[test]
fn user_agent_identifies_atomic() {
    assert!(eval("navigator.userAgent").contains("Atomic"));
}

#[test]
fn app_name_is_netscape() {
    assert_eq!(eval("navigator.appName"), "Netscape");
}

#[test]
fn platform_is_not_empty() {
    assert!(!eval("navigator.platform").is_empty());
}

#[test]
fn language_is_a_browser_safe_tag() {
    assert_eq!(
        eval("typeof navigator.language === 'string' && navigator.language.length >= 2"),
        "true"
    );
}

#[test]
fn languages_contains_the_primary_language() {
    assert_eq!(
        eval("navigator.languages.length >= 1 && typeof navigator.languages[0] === 'string'"),
        "true"
    );
}

#[test]
fn hardware_concurrency_is_positive() {
    assert_eq!(
        eval("typeof navigator.hardwareConcurrency === 'number' && navigator.hardwareConcurrency >= 1"),
        "true"
    );
}

#[test]
fn online_and_cookie_enabled_are_true() {
    assert_eq!(
        eval("navigator.onLine + ',' + navigator.cookieEnabled"),
        "true,true"
    );
}

#[test]
fn webdriver_is_false() {
    assert_eq!(eval("navigator.webdriver"), "false");
}

#[test]
fn clipboard_api_is_available() {
    assert_eq!(
        eval(
            "typeof navigator.clipboard.writeText === 'function' && typeof navigator.clipboard.readText === 'function'"
        ),
        "true"
    );
}
