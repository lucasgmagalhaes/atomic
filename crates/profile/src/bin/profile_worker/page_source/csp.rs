//! `collect_meta_csp_policies` — split out from `page_source.rs`.
//!
//! Also carries this worker's own `script-src` gating
//! ([`is_script_allowed`]) — `neutron::js::csp`'s own module doc
//! previously scoped CSP enforcement to `connect-src`/`default-src` only
//! ("this engine's `fetch`-family surface is the only thing CSP can
//! meaningfully restrict here, since there's no separate script-loading
//! ... policy hook to gate"). That was true when written: `load_scripts`
//! fetched every `<script src>` before a `Context` (and its CSP policies)
//! even existed. It's no longer true — every CSP policy this page will
//! ever have (`Content-Security-Policy` response headers, extracted in
//! `document_load::resolve_document`, plus every `<meta http-equiv>` tag
//! [`collect_meta_csp_policies`] already walks) is known right after
//! parsing, before `load_scripts` fetches anything — so gating can
//! happen here instead of needing a hook inside `js-runtime` at all.
//! Deliberately narrower than real spec's own `script-src`: gates only
//! whether an *external* `<script src>` is fetched (mirrors
//! `neutron::js::csp::is_connect_allowed`'s exact matching rules -
//! `*`/`'none'`/`'self'`/explicit origin or host, falling back to
//! `default-src`) - inline `<script>` content stays ungated, matching
//! `neutron::js::csp`'s own documented "no nonce/hash, no execution-gating
//! hook for inline content" scope cut.

use neutron::dom::{Dom, NodeData, NodeId};

/// Walks `node`'s subtree in document order collecting every
/// `<meta http-equiv="Content-Security-Policy" content="...">`'s policy
/// text (the `http-equiv` match is ASCII case-insensitive per the HTML
/// spec; an empty/missing `content` delivers no policy). Feeds the
/// `<meta>` half of CSP delivery - the response-header half is extracted
/// earlier, in `crate::document_load::resolve_document`, since only it
/// still holds the raw response. Scope cut vs a real browser's processing
/// model: this engine fully parses before running anything (there's no
/// streaming parser insertion-point timing to honor), so every delivered
/// policy - header or meta, wherever the tag sits - is simply in effect
/// for the whole page load, including its scripts.
pub(crate) fn collect_meta_csp_policies(dom: &Dom, node: NodeId, out: &mut Vec<String>) {
    let Some(n) = dom.get(node) else { return };
    if let NodeData::Element {
        tag, attributes, ..
    } = &n.data
    {
        if tag == "meta"
            && attributes
                .get("http-equiv")
                .is_some_and(|v| v.eq_ignore_ascii_case("content-security-policy"))
        {
            if let Some(policy) = attributes.get("content").filter(|p| !p.is_empty()) {
                out.push(policy.clone());
            }
        }
    }
    for &child in &n.children {
        collect_meta_csp_policies(dom, child, out);
    }
}

fn find_directive<'a>(policy: &'a str, name: &str) -> Option<Vec<&'a str>> {
    policy.split(';').find_map(|part| {
        let mut tokens = part.split_whitespace();
        let directive = tokens.next()?;
        directive
            .eq_ignore_ascii_case(name)
            .then(|| tokens.collect())
    })
}

/// Whether `script_url` may be fetched under a single `policy` — same
/// matching rules `neutron::js::csp::is_connect_allowed` already
/// documents for `connect-src` (`*`/`'none'`/`'self'`/explicit origin or
/// bare host), applied to `script-src` (falling back to `default-src`).
/// A policy with neither directive allows everything, matching real
/// CSP's "unspecified directive falls through to allow" default.
fn is_script_allowed_by(policy: &str, script_url: &str, page_origin: Option<&str>) -> bool {
    let Some(sources) =
        find_directive(policy, "script-src").or_else(|| find_directive(policy, "default-src"))
    else {
        return true;
    };
    let Ok(request) = url::Url::parse(script_url) else {
        return true;
    };
    for source in sources {
        let source = source.trim_matches('\'');
        match source {
            "none" => {}
            "*" => return true,
            "self" => {
                if page_origin
                    .is_some_and(|origin| origin == request.origin().ascii_serialization())
                {
                    return true;
                }
            }
            other => {
                if other == request.origin().ascii_serialization()
                    || request.host_str() == Some(other)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Whether `script_url` may be fetched under *every* delivered `policies`
/// entry — real CSP's multiple-policy model (policies intersect, so one
/// disallowing entry blocks regardless of the others), same convention
/// `neutron::js::csp::is_request_blocked` already uses for `connect-src`.
/// No policies at all: never blocks.
pub(crate) fn is_script_allowed(
    policies: &[String],
    script_url: &str,
    page_origin: Option<&str>,
) -> bool {
    policies
        .iter()
        .all(|policy| is_script_allowed_by(policy, script_url, page_origin))
}
