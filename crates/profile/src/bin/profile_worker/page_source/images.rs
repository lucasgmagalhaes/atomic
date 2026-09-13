//! `load_images` — split out from `page_source.rs`.

use std::collections::HashMap;
use std::rc::Rc;

use dom::{Dom, NodeData, NodeId};
use image_decode::DecodedImage;

use crate::network::ResourceCache;

use super::url::resolve_url;

/// Walks `node`'s subtree collecting every `<img src="...">`'s own
/// `NodeId` and raw (not yet resolved) `src`, in document order. Feeds
/// [`load_images`] — the real decode/sizing/painting primitives
/// (`image_decode`, `layout_engine::apply_image_sizes`, `render`'s
/// `build_image_list`/`composite_images`) already existed and were tested
/// at the crate level, but nothing in this worker ever called any of
/// them: a real navigated page's `<img>` rendered as an empty box, same
/// as `mockup/rendering-engine-gaps.md` §5 originally documented as a
/// "gap total" — this closes the missing wiring, not new primitives.
fn collect_image_sources(dom: &Dom, node: NodeId, out: &mut Vec<(NodeId, String)>) {
    let Some(n) = dom.get(node) else { return };
    if let NodeData::Element {
        tag, attributes, ..
    } = &n.data
    {
        if tag == "img" {
            if let Some(src) = attributes.get("src") {
                out.push((node, src.clone()));
            }
        }
    }
    for &child in &n.children {
        collect_image_sources(dom, child, out);
    }
}

/// Fetches and decodes every real `<img src>` in `dom` (rooted at `root`),
/// same "one real GET per resource, at load time" convention
/// `build_stylesheet` already uses for `<link>` stylesheets — `src`
/// resolved against `base_url` via [`resolve_url`], fetched through
/// [`fetch_with_cookies`] (so a profile's proxy/DNS/cookie jar apply to
/// images exactly like every other request this worker makes), decoded
/// via `image_decode::decode`. An unresolvable URL, a failed fetch, or
/// undecodable bytes (an unsupported format, corrupt data, an HTML error
/// page served with an `.jpg` extension) simply gets no entry — the same
/// "best-effort, never fail the whole page load over one bad resource"
/// stance `build_stylesheet` already takes, and `apply_image_sizes`
/// itself already documents as its contract for a missing entry.
pub(crate) fn load_images(
    dom: &Dom,
    root: NodeId,
    base_url: Option<&str>,
    storage_root: &std::path::Path,
    proxy: Option<&net::ProxyConfig>,
    dns_server: Option<std::net::SocketAddr>,
    cache: &mut ResourceCache,
) -> HashMap<NodeId, Rc<DecodedImage>> {
    let mut sources = Vec::new();
    collect_image_sources(dom, root, &mut sources);

    let mut images = HashMap::new();
    for (node, src) in sources {
        let Some(url) = resolve_url(base_url, &src) else {
            continue;
        };
        let Ok(response) = cache.fetch_cached(&url, storage_root, proxy, dns_server) else {
            continue;
        };
        let Some(decoded) = image_decode::decode(&response.body) else {
            continue;
        };
        images.insert(node, Rc::new(decoded));
    }
    images
}
