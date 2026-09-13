//! `serialize_line`/`deserialize_line` — the on-disk cookie-jar line
//! format — split out from `cookies.rs`.

use std::time::{Duration, UNIX_EPOCH};

use super::cookie::Cookie;
use super::same_site::SameSite;

/// One cookie per line, tab-separated fields (escaped via
/// [`crate::escape`], so a literal tab/newline in a value can't break
/// parsing): `name\tvalue\tdomain\tpath\texpires_epoch_or_dash\tsecure\thttp_only\tsame_site`.
pub(super) fn serialize_line(cookie: &Cookie) -> String {
    let expires = cookie
        .expires
        .map(|t| {
            t.duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string()
        })
        .unwrap_or_else(|| "-".to_string());
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        crate::escape(&cookie.name),
        crate::escape(&cookie.value),
        crate::escape(&cookie.domain),
        crate::escape(&cookie.path),
        expires,
        cookie.secure,
        cookie.http_only,
        cookie.same_site.as_str(),
    )
}

/// Accepts both the current 8-field format and the pre-`SameSite`
/// 7-field one (defaulting a legacy line's `same_site` to
/// [`SameSite::default`]) so an on-disk jar written before this field
/// existed still loads instead of silently dropping every cookie in it.
pub(super) fn deserialize_line(line: &str) -> Option<Cookie> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != 7 && fields.len() != 8 {
        return None;
    }
    let expires = if fields[4] == "-" {
        None
    } else {
        Some(UNIX_EPOCH + Duration::from_secs(fields[4].parse().ok()?))
    };
    let same_site = fields
        .get(7)
        .and_then(|s| SameSite::parse(s))
        .unwrap_or_default();
    Some(Cookie {
        name: crate::unescape(fields[0]),
        value: crate::unescape(fields[1]),
        domain: crate::unescape(fields[2]),
        path: crate::unescape(fields[3]),
        expires,
        secure: fields[5].parse().ok()?,
        http_only: fields[6].parse().ok()?,
        same_site,
    })
}
