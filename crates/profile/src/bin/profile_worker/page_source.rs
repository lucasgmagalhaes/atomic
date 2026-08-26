//! Pulling CSS/script/image *sources* out of a parsed DOM and turning them
//! into real, fetched, merged inputs for a `Page`: `<style>`/`<link>`
//! stylesheets (including `@import`), inline `<script>` text, `<meta
//! http-equiv="Content-Security-Policy">` policies, and `<img src>`
//! decoding.

use std::collections::HashMap;
use std::rc::Rc;

use css::{parse_stylesheet, Stylesheet};
use dom::{Dom, NodeData, NodeId};
use image_decode::DecodedImage;

use crate::network::fetch_with_cookies;

/// Applied to every page before anything the page's own `<style>`/
/// `<link>` sources contribute (lowest cascade priority - see
/// `build_stylesheet`) - only meaningfully matches the built-in demo
/// page's markup, but harmless (matches nothing) against a real one.
const BASE_STYLESHEET_SRC: &str = "#container { background-color: #1a1c2b; padding: 20px; } \
     p { color: #ffffff; font-size: 24px; }";

/// One CSS source found while walking a page's DOM, in document order.
enum CssSource {
    Inline(String),
    Link(String),
}

/// Walks `node`'s subtree collecting `<style>` text content and
/// `<link rel="stylesheet" href="...">` hrefs, in document order (a
/// document's later `<style>`/`<link>` should win over an earlier one at
/// equal specificity, same as real cascade order — preserving this order
/// is why this collects one interleaved list rather than two separate
/// ones).
fn collect_css_sources(dom: &Dom, node: NodeId, out: &mut Vec<CssSource>) {
    let Some(n) = dom.get(node) else { return };
    if let NodeData::Element {
        tag, attributes, ..
    } = &n.data
    {
        if tag == "style" {
            out.push(CssSource::Inline(dom.text_content(node)));
        } else if tag == "link" {
            let is_stylesheet = attributes
                .get("rel")
                .map(|r| r.eq_ignore_ascii_case("stylesheet"))
                .unwrap_or(false);
            if is_stylesheet {
                if let Some(href) = attributes.get("href") {
                    out.push(CssSource::Link(href.clone()));
                }
            }
        }
    }
    for &child in &n.children {
        collect_css_sources(dom, child, out);
    }
}

/// Real `<script>` tag execution - previously only the hardcoded demo
/// page's own `DEMO_SCRIPT` ever ran (`run_demo_script`, see `Page::load`);
/// a genuinely fetched/navigated page's own inline `<script>` content was
/// parsed into the DOM (as a real text-content child, same as any other
/// element) but never evaluated. Collects every `<script>` element's real
/// text content in document order (matches real script execution order -
/// a script placed after the elements it references, the common real
/// pattern, sees them already in the DOM by the time it runs).
///
/// Scope cut, same shape as `<link>` stylesheets' own history before this:
/// only inline `<script>...</script>` content - a `<script src="...">`
/// external script is a real further network-fetch feature, not attempted
/// in this pass, and its tag is walked past without effect (not an error).
pub(crate) fn collect_script_sources(dom: &Dom, node: NodeId, out: &mut Vec<String>) {
    let Some(n) = dom.get(node) else { return };
    if let NodeData::Element {
        tag, attributes, ..
    } = &n.data
    {
        if tag == "script" && !attributes.contains_key("src") {
            out.push(dom.text_content(node));
        }
    }
    for &child in &n.children {
        collect_script_sources(dom, child, out);
    }
}

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
) -> HashMap<NodeId, Rc<DecodedImage>> {
    let mut sources = Vec::new();
    collect_image_sources(dom, root, &mut sources);

    let mut images = HashMap::new();
    for (node, src) in sources {
        let Some(url) = resolve_url(base_url, &src) else {
            continue;
        };
        let Ok(response) = fetch_with_cookies(&url, storage_root, proxy, dns_server) else {
            continue;
        };
        let Some(decoded) = image_decode::decode(&response.body) else {
            continue;
        };
        images.insert(node, Rc::new(decoded));
    }
    images
}

/// Resolves a `<link href>` against `base_url` (the page's own URL - the
/// second argument to `Url::join`, exactly WHATWG's URL-resolution
/// algorithm via the real `url` crate: handles absolute hrefs,
/// scheme-relative `//host/...`, root-relative `/path`, and ordinary
/// relative `foo.css`/`../foo.css` alike, not just the "already absolute"
/// case this used to be limited to). Returns `None` for a non-`http(s)`
/// result (e.g. `data:`), a malformed href, or a relative href with no
/// `base_url` to resolve against (the built-in demo page has no URL of
/// its own).
fn resolve_url(base_url: Option<&str>, href: &str) -> Option<String> {
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

/// Parses `css_text`, fetches and merges any `@import`s it contains (an
/// import whose trailing media query doesn't match `viewport_width` is
/// skipped without fetching it at all — same "don't do the network work
/// for a rule that couldn't apply anyway" reasoning `<link media="...">`
/// would get if this crate parsed that attribute, which it doesn't yet),
/// then appends its own rules — imported rules land first, same relative
/// order a real `@import` (which must precede other rules) produces.
/// Only one level deep: an imported stylesheet's own `@import`s aren't
/// followed, to keep this bounded without needing cycle detection for
/// what's a real but rare case (a stylesheet importing a stylesheet that
/// imports another). `@import` hrefs resolve against the *page's* URL,
/// not the importing stylesheet's own URL (a real engine resolves
/// relative to whichever stylesheet contains the `@import`) — a
/// documented simplification, not a distinction this worker tracks.
fn merge_stylesheet_text(
    sheet: &mut Stylesheet,
    css_text: &str,
    base_url: Option<&str>,
    viewport_width: f64,
    storage_root: &std::path::Path,
    proxy: Option<&net::ProxyConfig>,
    dns_server: Option<std::net::SocketAddr>,
) {
    let parsed = parse_stylesheet(css_text);
    for import in &parsed.imports {
        let should_fetch = import
            .media
            .as_ref()
            .map(|m| m.matches(viewport_width))
            .unwrap_or(true);
        if !should_fetch {
            continue;
        }
        if let Some(resolved) = resolve_url(base_url, &import.url) {
            if let Ok(response) = fetch_with_cookies(&resolved, storage_root, proxy, dns_server) {
                let imported_text = String::from_utf8_lossy(&response.body).into_owned();
                sheet.rules.extend(parse_stylesheet(&imported_text).rules);
            }
        }
    }
    sheet.rules.extend(parsed.rules);
}

/// Resolves every CSS source in `dom` (rooted at `root`) into one cascaded
/// [`Stylesheet`]: [`BASE_STYLESHEET_SRC`] first, then each `<style>`/
/// `<link>` source in document order, `<link>` hrefs resolved against
/// `base_url` via [`resolve_url`] — a `<link>` that doesn't
/// resolve to a fetchable `http(s)://` URL is silently skipped, matching
/// `resolve_html`'s own "best-effort, never panics on a real page" stance
/// rather than failing the whole page load over one bad stylesheet
/// reference.
pub(crate) fn build_stylesheet(
    dom: &Dom,
    root: NodeId,
    base_url: Option<&str>,
    viewport_width: f64,
    storage_root: &std::path::Path,
    proxy: Option<&net::ProxyConfig>,
    dns_server: Option<std::net::SocketAddr>,
) -> Stylesheet {
    let mut sources = Vec::new();
    collect_css_sources(dom, root, &mut sources);

    let mut sheet = parse_stylesheet(BASE_STYLESHEET_SRC);
    for source in sources {
        let css_text = match source {
            CssSource::Inline(text) => Some(text),
            CssSource::Link(href) => resolve_url(base_url, &href)
                .and_then(|url| fetch_with_cookies(&url, storage_root, proxy, dns_server).ok())
                .map(|response| String::from_utf8_lossy(&response.body).into_owned()),
        };
        if let Some(css_text) = css_text {
            merge_stylesheet_text(
                &mut sheet,
                &css_text,
                base_url,
                viewport_width,
                storage_root,
                proxy,
                dns_server,
            );
        }
    }
    sheet
}
