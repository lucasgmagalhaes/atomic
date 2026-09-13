//! `parse_set_cookie` — split out from `cookies.rs`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::cookie::Cookie;
use super::date::parse_http_date;
use super::same_site::SameSite;

/// Parses one `Set-Cookie` header value, as received while fetching from
/// `request_host` (used as the default `Domain` when the header doesn't
/// specify one). Returns `None` for a header with no `name=value` pair.
pub fn parse_set_cookie(header: &str, request_host: &str) -> Option<Cookie> {
    let mut parts = header.split(';');
    let name_value = parts.next()?.trim();
    let (name, value) = name_value.split_once('=')?;
    if name.is_empty() {
        return None;
    }

    let mut cookie = Cookie {
        name: name.trim().to_string(),
        value: value.trim().to_string(),
        domain: request_host.to_string(),
        path: "/".to_string(),
        expires: None,
        secure: false,
        http_only: false,
        same_site: SameSite::default(),
    };

    let mut max_age: Option<i64> = None;
    for attr in parts {
        let attr = attr.trim();
        let (key, val) = attr.split_once('=').unwrap_or((attr, ""));
        match key.to_ascii_lowercase().as_str() {
            "domain" if !val.is_empty() => cookie.domain = val.trim().to_string(),
            "path" if !val.is_empty() => cookie.path = val.trim().to_string(),
            "max-age" => max_age = val.trim().parse().ok(),
            "expires" => cookie.expires = cookie.expires.or_else(|| parse_http_date(val.trim())),
            "secure" => cookie.secure = true,
            "httponly" => cookie.http_only = true,
            "samesite" => cookie.same_site = SameSite::parse(val).unwrap_or_default(),
            _ => {}
        }
    }

    // RFC 6265bis: a SameSite=None cookie must also be Secure, or a
    // real browser rejects it outright rather than storing an
    // effectively-cross-site cookie over plain HTTP.
    if cookie.same_site == SameSite::None && !cookie.secure {
        return None;
    }

    // Max-Age takes priority over Expires when both are present (RFC 6265
    // §5.3 step 3) — a negative or zero value marks the cookie for
    // deletion, expressed here as "already expired" rather than a special
    // case the jar has to know about separately.
    if let Some(max_age) = max_age {
        cookie.expires = Some(if max_age <= 0 {
            UNIX_EPOCH
        } else {
            SystemTime::now() + Duration::from_secs(max_age as u64)
        });
    }

    Some(cookie)
}
