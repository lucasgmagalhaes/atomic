use js_runtime::{Context, Runtime};

#[test]
fn navigator_user_agent_is_set() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("navigator.userAgent", "<test>");
    assert!(result.is_ok());
    let ua = result.unwrap();
    assert!(
        ua.contains("Atomic"),
        "userAgent should contain 'Atomic': {ua}"
    );
}

#[test]
fn navigator_app_name() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("navigator.appName", "<test>");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Netscape");
}

#[test]
fn navigator_platform() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("navigator.platform", "<test>");
    assert!(result.is_ok());
    let platform = result.unwrap();
    assert!(!platform.is_empty(), "platform should not be empty");
}

#[test]
fn navigator_language() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval(
        "(() => { \
            const l = navigator.language; \
            return typeof l === 'string' && l.length >= 2; \
        })()",
        "<test>",
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "true");
}

#[test]
fn navigator_languages_is_array() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval(
        "(() => { \
            return navigator.languages.length >= 1 && typeof navigator.languages[0] === 'string'; \
        })()",
        "<test>",
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "true");
}

#[test]
fn navigator_hardware_concurrency() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval(
        "(() => { \
            const c = navigator.hardwareConcurrency; \
            return typeof c === 'number' && c >= 1; \
        })()",
        "<test>",
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "true");
}

#[test]
fn navigator_online_cookie_enabled() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("navigator.onLine + ',' + navigator.cookieEnabled", "<test>");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "true,true");
}

#[test]
fn navigator_webdriver_false() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("navigator.webdriver", "<test>");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "false");
}

#[test]
fn navigator_clipboard_still_works() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval(
        "typeof navigator.clipboard.writeText === 'function' && typeof navigator.clipboard.readText === 'function'",
        "<test>",
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "true");
}

#[test]
fn screen_has_standard_properties() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval(
        "(() => { \
            const s = screen; \
            return typeof s.width === 'number' \
                && typeof s.height === 'number' \
                && typeof s.availWidth === 'number' \
                && typeof s.availHeight === 'number' \
                && typeof s.colorDepth === 'number' \
                && typeof s.pixelDepth === 'number' \
                && typeof s.orientation === 'object'; \
        })()",
        "<test>",
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "true");
}

#[test]
fn screen_width_and_height_are_positive() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("screen.width > 0 && screen.height > 0", "<test>");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "true");
}

#[test]
fn screen_orientation_has_type_and_angle() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval(
        "(() => { \
            const o = screen.orientation; \
            return typeof o.type === 'string' && typeof o.angle === 'number'; \
        })()",
        "<test>",
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "true");
}
