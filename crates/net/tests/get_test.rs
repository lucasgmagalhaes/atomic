#[test]
fn gets_a_real_https_url() {
    let response = net::get("https://example.com/").expect("request should succeed");
    assert_eq!(response.status, 200);
    let body = String::from_utf8_lossy(&response.body);
    assert!(
        body.contains("Example Domain"),
        "body should contain example.com's known content"
    );
}

#[test]
fn rejects_an_invalid_url() {
    let result = net::get("not a url");
    assert!(matches!(result, Err(net::Error::InvalidUrl(_))));
}

#[test]
fn plain_http_works_too() {
    let response = net::get("http://example.com/").expect("plain HTTP request should succeed");
    assert_eq!(response.status, 200);
}

#[test]
fn captures_response_headers() {
    let response = net::get("https://example.com/").expect("request should succeed");
    assert!(
        response
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type")),
        "expected a content-type header, got: {:?}",
        response.headers
    );
}

#[test]
fn sends_extra_request_headers() {
    let response = net::get_with_headers(
        "https://httpbin.org/headers",
        &[("X-Atomic-Test", "hello-atomic")],
    )
    .expect("request should succeed");
    let body = String::from_utf8_lossy(&response.body);
    assert!(
        body.contains("hello-atomic"),
        "expected the custom header echoed back, got: {body}"
    );
}
