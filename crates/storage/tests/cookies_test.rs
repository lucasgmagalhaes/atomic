use std::time::{Duration, SystemTime};

use storage::cookies::{parse_set_cookie, CookieJar, SameSite};

fn temp_path(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nimble-cookies-test-{tag}-{nanos}.txt"))
}

#[test]
fn parses_a_basic_set_cookie() {
    let cookie = parse_set_cookie("session=abc123", "example.com").unwrap();
    assert_eq!(cookie.name, "session");
    assert_eq!(cookie.value, "abc123");
    assert_eq!(cookie.domain, "example.com");
    assert_eq!(cookie.path, "/");
    assert_eq!(cookie.expires, None);
    assert!(!cookie.secure);
    assert!(!cookie.http_only);
}

#[test]
fn parses_attributes() {
    let cookie = parse_set_cookie(
        "id=42; Domain=sub.example.com; Path=/app; Secure; HttpOnly; Max-Age=3600",
        "example.com",
    )
    .unwrap();
    assert_eq!(cookie.domain, "sub.example.com");
    assert_eq!(cookie.path, "/app");
    assert!(cookie.secure);
    assert!(cookie.http_only);
    let expires = cookie.expires.unwrap();
    let now_plus_hour = SystemTime::now() + Duration::from_secs(3600);
    assert!(expires <= now_plus_hour && expires > SystemTime::now());
}

#[test]
fn parses_the_rfc1123_expires_date() {
    let cookie =
        parse_set_cookie("a=1; Expires=Wed, 21 Oct 2015 07:28:00 GMT", "example.com").unwrap();
    let expires = cookie
        .expires
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(expires, 1_445_412_480);
}

#[test]
fn max_age_zero_marks_the_cookie_already_expired() {
    let cookie = parse_set_cookie("a=1; Max-Age=0", "example.com").unwrap();
    assert_eq!(cookie.expires, Some(std::time::UNIX_EPOCH));
}

#[test]
fn returns_none_without_a_name_value_pair() {
    assert!(parse_set_cookie("", "example.com").is_none());
    assert!(parse_set_cookie("justaname", "example.com").is_none());
}

#[test]
fn jar_matches_by_domain_path_and_scheme() {
    let path = temp_path("match");
    let mut jar = CookieJar::open(&path).unwrap();
    jar.set_from_header("a=1; Domain=example.com; Path=/app", "example.com")
        .unwrap();
    jar.set_from_header("b=2; Secure", "example.com").unwrap();

    // wrong path
    assert!(jar
        .matching("example.com", "/other", true)
        .iter()
        .all(|c| c.name != "a"));
    // right path
    assert!(jar
        .matching("example.com", "/app/page", true)
        .iter()
        .any(|c| c.name == "a"));
    // subdomain matches a Domain=example.com cookie
    assert!(jar
        .matching("www.example.com", "/", true)
        .iter()
        .any(|c| c.name == "b"));
    // secure cookie excluded from a plain-http context
    assert!(jar
        .matching("example.com", "/", false)
        .iter()
        .all(|c| c.name != "b"));
}

#[test]
fn header_value_joins_matching_cookies() {
    let path = temp_path("header");
    let mut jar = CookieJar::open(&path).unwrap();
    jar.set_from_header("a=1", "example.com").unwrap();
    jar.set_from_header("b=2", "example.com").unwrap();

    let header = jar.header_value("example.com", "/", true).unwrap();
    assert!(header.contains("a=1"));
    assert!(header.contains("b=2"));
    assert!(jar.header_value("other.com", "/", true).is_none());
}

#[test]
fn set_replaces_a_cookie_with_the_same_name_domain_and_path() {
    let path = temp_path("replace");
    let mut jar = CookieJar::open(&path).unwrap();
    jar.set_from_header("a=1", "example.com").unwrap();
    jar.set_from_header("a=2", "example.com").unwrap();

    assert_eq!(jar.len(), 1);
    assert_eq!(jar.header_value("example.com", "/", true).unwrap(), "a=2");
}

#[test]
fn expired_cookies_are_persisted_but_never_matched() {
    let path = temp_path("expired");
    let mut jar = CookieJar::open(&path).unwrap();
    jar.set_from_header("a=1; Max-Age=0", "example.com")
        .unwrap();

    assert_eq!(jar.len(), 1);
    assert!(jar.matching("example.com", "/", true).is_empty());
}

#[test]
fn clear_expired_removes_them_for_good() {
    let path = temp_path("clear-expired");
    let mut jar = CookieJar::open(&path).unwrap();
    jar.set_from_header("a=1; Max-Age=0", "example.com")
        .unwrap();
    jar.set_from_header("b=2", "example.com").unwrap();

    jar.clear_expired().unwrap();
    assert_eq!(jar.len(), 1);
}

#[test]
fn persists_across_separate_open_calls() {
    let path = temp_path("persist");
    {
        let mut jar = CookieJar::open(&path).unwrap();
        jar.set_from_header(
            "a=1; Domain=example.com; Path=/app; Secure; HttpOnly",
            "example.com",
        )
        .unwrap();
    }
    let reopened = CookieJar::open(&path).unwrap();
    assert_eq!(
        reopened.header_value("example.com", "/app", true).unwrap(),
        "a=1"
    );
}

#[test]
fn remove_deletes_a_single_cookie() {
    let path = temp_path("remove");
    let mut jar = CookieJar::open(&path).unwrap();
    jar.set_from_header("a=1", "example.com").unwrap();
    jar.remove("a", "example.com", "/").unwrap();
    assert!(jar.is_empty());
}

#[test]
fn same_site_defaults_to_lax_when_unspecified() {
    let cookie = parse_set_cookie("a=1", "example.com").unwrap();
    assert_eq!(cookie.same_site, SameSite::Lax);
}

#[test]
fn same_site_attribute_is_parsed_case_insensitively() {
    assert_eq!(
        parse_set_cookie("a=1; SameSite=Strict", "example.com")
            .unwrap()
            .same_site,
        SameSite::Strict
    );
    assert_eq!(
        parse_set_cookie("a=1; samesite=lax", "example.com")
            .unwrap()
            .same_site,
        SameSite::Lax
    );
    assert_eq!(
        parse_set_cookie("a=1; SameSite=None; Secure", "example.com")
            .unwrap()
            .same_site,
        SameSite::None
    );
}

#[test]
fn same_site_none_without_secure_is_rejected() {
    assert!(parse_set_cookie("a=1; SameSite=None", "example.com").is_none());
}

#[test]
fn an_unrecognized_same_site_value_falls_back_to_the_default() {
    assert_eq!(
        parse_set_cookie("a=1; SameSite=bogus", "example.com")
            .unwrap()
            .same_site,
        SameSite::Lax
    );
}

#[test]
fn matching_with_context_excludes_strict_and_lax_cookies_from_a_cross_site_request() {
    let path = temp_path("same-site-strict");
    let mut jar = CookieJar::open(&path).unwrap();
    jar.set_from_header("a=1; SameSite=Strict", "example.com")
        .unwrap();
    jar.set_from_header("b=2; SameSite=Lax", "example.com")
        .unwrap();

    assert!(jar
        .matching_with_context("example.com", "/", true, false)
        .is_empty());
    assert_eq!(
        jar.matching_with_context("example.com", "/", true, true)
            .len(),
        2
    );
}

#[test]
fn matching_with_context_still_sends_same_site_none_cross_site() {
    let path = temp_path("same-site-none");
    let mut jar = CookieJar::open(&path).unwrap();
    jar.set_from_header("a=1; SameSite=None; Secure", "example.com")
        .unwrap();

    let cross_site = jar.matching_with_context("example.com", "/", true, false);
    assert_eq!(cross_site.len(), 1);
    assert_eq!(cross_site[0].name, "a");
}

#[test]
fn plain_matching_and_header_value_are_unaffected_by_same_site() {
    let path = temp_path("same-site-plain-api");
    let mut jar = CookieJar::open(&path).unwrap();
    jar.set_from_header("a=1; SameSite=Strict", "example.com")
        .unwrap();
    // The plain (no-context) API always behaves as same-site, so a
    // Strict cookie still comes back - unchanged from before SameSite
    // enforcement existed.
    assert_eq!(jar.header_value("example.com", "/", true).unwrap(), "a=1");
}

#[test]
fn same_site_persists_across_separate_open_calls() {
    let path = temp_path("same-site-persist");
    {
        let mut jar = CookieJar::open(&path).unwrap();
        jar.set_from_header("a=1; SameSite=Strict", "example.com")
            .unwrap();
    }
    let reopened = CookieJar::open(&path).unwrap();
    assert!(reopened
        .matching_with_context("example.com", "/", true, false)
        .is_empty());
    assert_eq!(
        reopened
            .matching_with_context("example.com", "/", true, true)
            .len(),
        1
    );
}
