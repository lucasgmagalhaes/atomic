//! A real, narrowly-scoped `Content-Security-Policy` primitive: parses a
//! policy string and answers "is this fetch destination allowed" against
//! its `connect-src` (falling back to `default-src`, per spec) directive —
//! the one CSP directive that actually governs this engine's `fetch`/
//! `fetchSync`/`XMLHttpRequest` targets, same "only enforce what this
//! engine can actually request" scope `cors.rs` already documents for
//! CORS/mixed-content.
//!
//! Source-expression support is deliberately small: `*` (allow any),
//! `'none'` (allow nothing), `'self'` (matches the page's own origin,
//! same value [`crate::cors::page_origin`] computes), and an explicit
//! origin or bare host (`https://api.example.com`, `api.example.com`).
//! Not implemented: nonces/hashes (`'nonce-...'`/`'sha256-...'` — those
//! govern inline `<script>`/`<style>` content, which this engine has no
//! CSP-relevant execution-gating hook for anyway), wildcards within a host
//! (`*.example.com`), scheme-only sources (`https:`), and every
//! non-`connect-src`/`default-src` directive (`script-src`, `img-src`,
//! ... — this engine's `fetch`-family surface is the only thing CSP can
//! meaningfully restrict here, since there's no separate script-loading or
//! image-fetch policy hook to gate).
//!
//! Wiring gap, documented rather than half-done: nothing in this crate or
//! `profile-worker` extracts a real `Content-Security-Policy` HTTP
//! response header or `<meta http-equiv="Content-Security-Policy">` tag
//! and calls [`crate::Context::set_csp`] yet — a host that wants real
//! enforcement has to call it itself (same shape `set_url`/
//! `set_computed_styles` already have: a real, tested primitive a host
//! wires in when ready, not a fake one).

use quickjs_sys as sys;

/// `true` if `request_url` should be blocked before it's even sent — the
/// same boolean-blocked convention `cors::is_mixed_content_blocked`
/// already uses at every one of `fetch`/`fetchSync`/`XMLHttpRequest`'s
/// call sites, so this slots in right next to it. A context with no CSP
/// set (`HostState.csp: None`) never blocks.
pub(crate) unsafe fn is_request_blocked(ctx: *mut sys::JSContext, request_url: &str) -> bool {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return false;
    }
    let Some(policy) = (*state).csp.as_deref() else {
        return false;
    };
    let page_origin = crate::cors::page_origin(ctx);
    !is_connect_allowed(policy, request_url, page_origin.as_deref())
}

fn find_directive<'a>(policy: &'a str, name: &str) -> Option<Vec<&'a str>> {
    policy.split(';').find_map(|part| {
        let mut tokens = part.split_whitespace();
        let directive = tokens.next()?;
        directive.eq_ignore_ascii_case(name).then(|| tokens.collect())
    })
}

/// Whether `request_url` may be fetched under `policy`, given the page's
/// own origin (from [`crate::cors::page_origin`]) for `'self'` matching.
/// A policy with no `connect-src`/`default-src` directive at all allows
/// everything (nothing restricts it) — matches real CSP's "unspecified
/// directive falls through to allow" default.
pub(crate) fn is_connect_allowed(policy: &str, request_url: &str, page_origin: Option<&str>) -> bool {
    let Some(sources) = find_directive(policy, "connect-src").or_else(|| find_directive(policy, "default-src")) else {
        return true;
    };
    let Ok(request) = url::Url::parse(request_url) else {
        return true;
    };
    for source in sources {
        let source = source.trim_matches('\'');
        match source {
            "none" => {}
            "*" => return true,
            "self" => {
                if page_origin.is_some_and(|origin| origin == request.origin().ascii_serialization()) {
                    return true;
                }
            }
            other => {
                if other == request.origin().ascii_serialization() || request.host_str() == Some(other) {
                    return true;
                }
            }
        }
    }
    false
}
