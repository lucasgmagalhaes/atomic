//! `resolve_url` — split out from `page_source.rs`.

/// Resolves a `<link href>` against `base_url` (the page's own URL - the
/// second argument to `Url::join`, exactly WHATWG's URL-resolution
/// algorithm via the real `url` crate: handles absolute hrefs,
/// scheme-relative `//host/...`, root-relative `/path`, and ordinary
/// relative `foo.css`/`../foo.css` alike, not just the "already absolute"
/// case this used to be limited to). Returns `None` for a non-`http(s)`
/// result (e.g. `data:`), a malformed href, or a relative href with no
/// `base_url` to resolve against (the built-in demo page has no URL of
/// its own).
pub(crate) fn resolve_url(base_url: Option<&str>, href: &str) -> Option<String> {
    let resolved = match base_url {
        Some(base) => url::Url::parse(base).ok()?.join(href).ok()?,
        None => url::Url::parse(href).ok()?,
    };
    if resolved.scheme() == "http" || resolved.scheme() == "https" {
        Some(resolved.to_string())
    } else {
        None
    }
}
