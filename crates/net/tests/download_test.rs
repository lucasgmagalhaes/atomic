use std::fs;

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("nimble-net-download-test-{}-{name}", std::process::id()))
}

#[test]
fn download_writes_the_real_response_body_to_disk() {
    let dest = temp_path("example.html");
    let response = net::download("https://example.com/", &dest).expect("download should succeed");
    assert_eq!(response.status, 200);
    assert!(response.body.is_empty(), "body should already be flushed to disk, not held in memory");

    let contents = fs::read_to_string(&dest).expect("file should exist and be readable");
    assert!(contents.contains("Example Domain"));

    fs::remove_file(&dest).ok();
}

#[test]
fn download_overwrites_an_existing_file() {
    let dest = temp_path("overwrite.html");
    fs::write(&dest, b"stale content").unwrap();

    net::download("https://example.com/", &dest).expect("download should succeed");
    let contents = fs::read_to_string(&dest).unwrap();
    assert!(contents.contains("Example Domain"));
    assert!(!contents.contains("stale content"));

    fs::remove_file(&dest).ok();
}

#[test]
fn download_reports_response_headers() {
    let dest = temp_path("headers.html");
    let response = net::download("https://example.com/", &dest).expect("download should succeed");
    assert!(response.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")));

    fs::remove_file(&dest).ok();
}

#[test]
fn download_fails_on_an_invalid_url_without_touching_the_filesystem() {
    let dest = temp_path("should-not-exist.html");
    let result = net::download("not a url", &dest);
    assert!(matches!(result, Err(net::Error::InvalidUrl(_))));
    assert!(!dest.exists());
}

#[test]
fn download_with_headers_sends_extra_request_headers() {
    let dest = temp_path("headers-echo.json");
    net::download_with_headers("https://httpbin.org/headers", &[("X-Nimble-Test", "hello-nimble")], &dest)
        .expect("download should succeed");
    let contents = fs::read_to_string(&dest).unwrap();
    assert!(contents.contains("hello-nimble"));

    fs::remove_file(&dest).ok();
}
