//! `document.cookie`-shaped cookie jar: real `Set-Cookie` parsing (name,
//! value, `Domain`, `Path`, `Max-Age`, `Expires`, `Secure`, `HttpOnly`),
//! real domain/path/expiry matching for building an outgoing `Cookie`
//! header, and real file persistence (same one-mutation-one-flush pattern
//! as [`crate::LocalStorage`]).
//!
//! `SameSite` (`Strict`/`Lax`/`None`) is now real: parsed from
//! `Set-Cookie` (defaulting to `Lax`, matching modern browsers' own
//! unspecified-`SameSite` default), stored, and enforced by
//! [`CookieJar::matching_with_context`]/[`CookieJar::header_value_with_context`]
//! against an explicit `request_is_same_site` flag a caller supplies — a
//! `SameSite=None` cookie set without `Secure` is rejected outright at
//! parse time (`parse_set_cookie` returns `None`), matching real browser
//! behavior. The plain [`CookieJar::matching`]/[`CookieJar::header_value`]
//! (used by every caller in this workspace today) are unchanged thin
//! wrappers that always pass `request_is_same_site: true` — this
//! workspace has no cross-origin embedding (no iframes exist in `dom` at
//! all) and already partitions cookie storage per request host one
//! directory per origin, so there is no cross-site cookie flow for the
//! policy to meaningfully block yet; the enforcement exists and is
//! tested at the crate level, ready for whatever caller first needs a
//! real cross-site request context. One deliberate simplification
//! against the full spec: `Lax` is enforced identically to `Strict` here
//! (both require `request_is_same_site: true`) rather than additionally
//! allowing top-level cross-site GET navigations, since nothing in this
//! workspace distinguishes a top-level navigation from a subresource
//! fetch yet.
//!
//! Other deviations from RFC 6265: no public-suffix-list-aware domain checks (a
//! cookie for `Domain=co.uk` would wrongly match every `co.uk` subdomain —
//! the PSL is a large, frequently-updated external dataset, out of scope
//! here), and `Expires`' HTTP-date is parsed by a small hand-rolled parser
//! (RFC 1123's fixed six-token format only, via the standard
//! days-since-epoch civil calendar algorithm — no crate dependency for
//! one date format).
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl Default for SameSite {
    /// Modern browsers treat an unspecified `SameSite` as `Lax`, not
    /// "no restriction" — matched here rather than defaulting to `None`.
    fn default() -> Self {
        SameSite::Lax
    }
}

impl SameSite {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "strict" => Some(SameSite::Strict),
            "lax" => Some(SameSite::Lax),
            "none" => Some(SameSite::None),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            SameSite::Strict => "strict",
            SameSite::Lax => "lax",
            SameSite::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    /// `None` = session cookie (no `Max-Age`/`Expires` given).
    pub expires: Option<SystemTime>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
}

impl Cookie {
    fn is_expired(&self, now: SystemTime) -> bool {
        matches!(self.expires, Some(exp) if exp <= now)
    }

    /// RFC 6265 §5.1.3 domain matching: exact match, or `host` is a
    /// subdomain of `self.domain` (a leading-dot-normalized `Domain`
    /// attribute already applies to subdomains, and browsers apply the
    /// same rule even without the leading dot in practice — matched here).
    fn domain_matches(&self, host: &str) -> bool {
        let domain = self.domain.trim_start_matches('.');
        host.eq_ignore_ascii_case(domain) || host.to_ascii_lowercase().ends_with(&format!(".{}", domain.to_ascii_lowercase()))
    }

    /// RFC 6265 §5.1.4 default-path-style matching: `self.path` must be a
    /// prefix of `request_path`, ending exactly at a `/` boundary (or the
    /// whole path).
    fn path_matches(&self, request_path: &str) -> bool {
        if !request_path.starts_with(&self.path) {
            return false;
        }
        self.path == "/" || request_path.len() == self.path.len() || request_path.as_bytes()[self.path.len()] == b'/'
    }
}

fn month_index(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    MONTHS.iter().position(|m| m.eq_ignore_ascii_case(name)).map(|i| i as u32)
}

/// Howard Hinnant's `days_from_civil`: days since the Unix epoch for a
/// given proleptic-Gregorian calendar date. Public-domain algorithm, no
/// leap-second/timezone handling needed since HTTP dates are always GMT.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Parses an RFC 1123 HTTP-date: `"Wdy, DD Mon YYYY HH:MM:SS GMT"`.
fn parse_http_date(s: &str) -> Option<SystemTime> {
    let parts: Vec<&str> = s.trim().split_whitespace().collect();
    if parts.len() != 6 {
        return None;
    }
    let day: u32 = parts[1].parse().ok()?;
    let month = month_index(parts[2])?;
    let year: i64 = parts[3].parse().ok()?;
    let mut time = parts[4].split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let min: i64 = time.next()?.parse().ok()?;
    let sec: i64 = time.next()?.parse().ok()?;

    let days = days_from_civil(year, month + 1, day);
    let epoch_secs = days * 86400 + hour * 3600 + min * 60 + sec;
    if epoch_secs < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(epoch_secs as u64))
}

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

/// A cookie jar for one profile, persisted to one file — real matching
/// against a request's host/path/scheme, real file I/O, no in-memory-only
/// mode.
pub struct CookieJar {
    path: PathBuf,
    cookies: Vec<Cookie>,
}

impl CookieJar {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut cookies = Vec::new();

        if let Ok(contents) = std::fs::read_to_string(&path) {
            for line in contents.lines() {
                if let Some(cookie) = deserialize_line(line) {
                    cookies.push(cookie);
                }
            }
        }

        Ok(CookieJar { path, cookies })
    }

    /// Inserts or replaces `cookie` — identified by (name, domain, path)
    /// per RFC 6265, not by name alone, so `a=1` scoped to `/foo` and `a=1`
    /// scoped to `/bar` coexist. A cookie whose `expires` is already in the
    /// past is stored anyway (matches `set_from_header`'s deletion-via-
    /// Max-Age=0 path) but will simply never be returned by
    /// [`matching`](Self::matching) and is swept away by
    /// [`clear_expired`](Self::clear_expired).
    pub fn set(&mut self, cookie: Cookie) -> io::Result<()> {
        self.cookies.retain(|c| !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path));
        self.cookies.push(cookie);
        self.flush()
    }

    /// Parses `header` (a `Set-Cookie` value) as received from
    /// `request_host` and stores it.
    pub fn set_from_header(&mut self, header: &str, request_host: &str) -> io::Result<()> {
        if let Some(cookie) = parse_set_cookie(header, request_host) {
            self.set(cookie)?;
        }
        Ok(())
    }

    /// Every non-expired cookie whose domain/path/secure-ness matches a
    /// request to `host` + `path` under `secure_context` (true for
    /// `https://`). Always same-site (see module docs) - equivalent to
    /// [`matching_with_context`](Self::matching_with_context) with
    /// `request_is_same_site: true`.
    pub fn matching(&self, host: &str, path: &str, secure_context: bool) -> Vec<&Cookie> {
        self.matching_with_context(host, path, secure_context, true)
    }

    /// Same as [`matching`](Self::matching), plus real `SameSite`
    /// enforcement: a `Strict`/`Lax` cookie is excluded unless
    /// `request_is_same_site` is `true` (see module docs for why `Lax`
    /// isn't given its usual "top-level navigation" exception here); a
    /// `None` cookie is never excluded on this basis.
    pub fn matching_with_context(&self, host: &str, path: &str, secure_context: bool, request_is_same_site: bool) -> Vec<&Cookie> {
        let now = SystemTime::now();
        self.cookies
            .iter()
            .filter(|c| {
                !c.is_expired(now)
                    && c.domain_matches(host)
                    && c.path_matches(path)
                    && (!c.secure || secure_context)
                    && (request_is_same_site || c.same_site == SameSite::None)
            })
            .collect()
    }

    /// Builds a `Cookie` request header value (`"a=1; b=2"`) from
    /// [`matching`](Self::matching), or `None` if nothing matches.
    pub fn header_value(&self, host: &str, path: &str, secure_context: bool) -> Option<String> {
        self.header_value_with_context(host, path, secure_context, true)
    }

    /// Same as [`header_value`](Self::header_value), built from
    /// [`matching_with_context`](Self::matching_with_context) instead.
    pub fn header_value_with_context(&self, host: &str, path: &str, secure_context: bool, request_is_same_site: bool) -> Option<String> {
        let matches = self.matching_with_context(host, path, secure_context, request_is_same_site);
        if matches.is_empty() {
            return None;
        }
        Some(matches.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; "))
    }

    pub fn remove(&mut self, name: &str, domain: &str, path: &str) -> io::Result<()> {
        self.cookies.retain(|c| !(c.name == name && c.domain == domain && c.path == path));
        self.flush()
    }

    pub fn clear_expired(&mut self) -> io::Result<()> {
        let now = SystemTime::now();
        self.cookies.retain(|c| !c.is_expired(now));
        self.flush()
    }

    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    fn flush(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = String::new();
        for cookie in &self.cookies {
            out.push_str(&serialize_line(cookie));
            out.push('\n');
        }
        std::fs::write(&self.path, out)
    }
}

/// One cookie per line, tab-separated fields (escaped via
/// [`crate::escape`], so a literal tab/newline in a value can't break
/// parsing): `name\tvalue\tdomain\tpath\texpires_epoch_or_dash\tsecure\thttp_only\tsame_site`.
fn serialize_line(cookie: &Cookie) -> String {
    let expires = cookie
        .expires
        .map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs().to_string())
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
fn deserialize_line(line: &str) -> Option<Cookie> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != 7 && fields.len() != 8 {
        return None;
    }
    let expires = if fields[4] == "-" {
        None
    } else {
        Some(UNIX_EPOCH + Duration::from_secs(fields[4].parse().ok()?))
    };
    let same_site = fields.get(7).and_then(|s| SameSite::parse(s)).unwrap_or_default();
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
