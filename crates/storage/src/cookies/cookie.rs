//! `Cookie` — split out from `cookies.rs`.

use std::time::SystemTime;

use super::same_site::SameSite;

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
    pub(super) fn is_expired(&self, now: SystemTime) -> bool {
        matches!(self.expires, Some(exp) if exp <= now)
    }

    /// RFC 6265 §5.1.3 domain matching: exact match, or `host` is a
    /// subdomain of `self.domain` (a leading-dot-normalized `Domain`
    /// attribute already applies to subdomains, and browsers apply the
    /// same rule even without the leading dot in practice — matched here).
    pub(super) fn domain_matches(&self, host: &str) -> bool {
        let domain = self.domain.trim_start_matches('.');
        host.eq_ignore_ascii_case(domain)
            || host
                .to_ascii_lowercase()
                .ends_with(&format!(".{}", domain.to_ascii_lowercase()))
    }

    /// RFC 6265 §5.1.4 default-path-style matching: `self.path` must be a
    /// prefix of `request_path`, ending exactly at a `/` boundary (or the
    /// whole path).
    pub(super) fn path_matches(&self, request_path: &str) -> bool {
        if !request_path.starts_with(&self.path) {
            return false;
        }
        self.path == "/"
            || request_path.len() == self.path.len()
            || request_path.as_bytes()[self.path.len()] == b'/'
    }
}
