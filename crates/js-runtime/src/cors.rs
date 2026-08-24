//! Real (though scoped) same-origin/CORS/mixed-content enforcement — the
//! "origin/security model" `JS_ENGINE_CAPABILITY_MATRIX.md` calls out as a
//! prerequisite before expanding cross-origin networking any further.
//!
//! This engine's `fetch`/`fetchSync`/`XMLHttpRequest` only ever issue a
//! plain, credential-less GET with no caller-supplied custom headers — a
//! real Fetch spec "simple request", which never triggers a CORS
//! preflight (`OPTIONS`) even in a real browser, since a plain `<img>`/
//! `<script>`/`<link>` tag could already provoke the same request from any
//! page. So the enforcement needed here is exactly the *response* check a
//! simple cross-origin request still gets: same-origin is always allowed;
//! a cross-origin response only reaches script if it carries a real
//! `Access-Control-Allow-Origin` header that is `*` or matches the
//! requesting page's own origin exactly. No `Access-Control-Allow-
//! Credentials`, no multi-origin allowlist parsing, no `Vary` handling —
//! real further refinements this engine doesn't attempt, matching its
//! already-minimal fetch scope (GET-only, no request headers/body).
use quickjs_sys as sys;

/// The current page's own origin (`scheme://host[:port]`), or `None` if
/// there's no real navigated URL to derive one from (a plain
/// `Context::with_dom`/test, or `profile-worker`'s built-in demo page) —
/// callers treat `None` as "nothing to enforce against" (see
/// [`is_response_allowed`]), the same degrade-gracefully pattern
/// `location`'s own properties already use for the same missing state.
pub(crate) unsafe fn page_origin(ctx: *mut sys::JSContext) -> Option<String> {
    crate::location::current_url(ctx).map(|u| u.origin().ascii_serialization())
}

/// Whether a response fetched from `request_url` may reach script, given
/// the page's own `page_origin` (from [`page_origin`]) and the response's
/// real headers. Same-origin requests are always allowed. `page_origin:
/// None` also always allows (nothing to enforce — see this module's doc).
/// An unparseable `request_url` allows too, since malformed-URL handling
/// already happens earlier in the fetch path (a request that never made it
/// this far can't be a CORS bypass).
pub(crate) fn is_response_allowed(page_origin: Option<&str>, request_url: &str, response_headers: &[(String, String)]) -> bool {
    let Some(page_origin) = page_origin else {
        return true;
    };
    let Ok(request) = url::Url::parse(request_url) else {
        return true;
    };
    if request.origin().ascii_serialization() == page_origin {
        return true;
    }
    response_headers.iter().any(|(name, value)| {
        if !name.eq_ignore_ascii_case("access-control-allow-origin") {
            return false;
        }
        let value = value.trim();
        value == "*" || value == page_origin
    })
}

/// Real mixed-content blocking: a page loaded over `https://` must never
/// fetch a plain `http://` subresource — a real browser blocks this
/// outright, *before* even sending the request (unlike CORS, which sends
/// the request but hides the response from script). `page_origin: None`
/// (nothing to enforce — see [`page_origin`]) and a page whose own origin
/// isn't `https://` both never block, matching a real browser's own scope
/// (mixed-content blocking only exists to protect an otherwise-secure
/// page). No distinction between "active" (script, fetch) and "passive"
/// (image) mixed content — real browsers treat passive content more
/// leniently, but this engine's `fetch`/`fetchSync`/`XMLHttpRequest` are
/// all fetch-shaped requests a script initiated, closer in spirit to
/// active content, so blocking all of them uniformly is the simpler,
/// correct-enough default. `<img>` fetching (`profile-worker`'s own
/// `net::get` calls for page resources) doesn't go through this at all —
/// scoped to script-initiated requests only.
pub(crate) fn is_mixed_content_blocked(page_origin: Option<&str>, request_url: &str) -> bool {
    let Some(page_origin) = page_origin else {
        return false;
    };
    if !page_origin.starts_with("https://") {
        return false;
    }
    url::Url::parse(request_url).map(|u| u.scheme() == "http").unwrap_or(false)
}
