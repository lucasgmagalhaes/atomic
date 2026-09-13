//! `collect_meta_csp_policies` — split out from `page_source.rs`.

use dom::{Dom, NodeData, NodeId};

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
