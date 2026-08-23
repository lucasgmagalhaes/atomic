#[test]
fn gets_a_real_https_url() {
    let response = net::get("https://example.com/").expect("request should succeed");
    assert_eq!(response.status, 200);
    let body = String::from_utf8_lossy(&response.body);
    assert!(body.contains("Example Domain"), "body should contain example.com's known content");
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
