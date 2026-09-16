use js_runtime::{Context, Runtime};

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
