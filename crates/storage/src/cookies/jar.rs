//! `CookieJar` — split out from `cookies.rs`.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::cookie::Cookie;
use super::parse::parse_set_cookie;
use super::persist::{deserialize_line, serialize_line};
use super::same_site::SameSite;

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
        self.cookies.retain(|c| {
            !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path)
        });
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
    pub fn matching_with_context(
        &self,
        host: &str,
        path: &str,
        secure_context: bool,
        request_is_same_site: bool,
    ) -> Vec<&Cookie> {
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
    pub fn header_value_with_context(
        &self,
        host: &str,
        path: &str,
        secure_context: bool,
        request_is_same_site: bool,
    ) -> Option<String> {
        let matches = self.matching_with_context(host, path, secure_context, request_is_same_site);
        if matches.is_empty() {
            return None;
        }
        Some(
            matches
                .iter()
                .map(|c| format!("{}={}", c.name, c.value))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    pub fn remove(&mut self, name: &str, domain: &str, path: &str) -> io::Result<()> {
        self.cookies
            .retain(|c| !(c.name == name && c.domain == domain && c.path == path));
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
