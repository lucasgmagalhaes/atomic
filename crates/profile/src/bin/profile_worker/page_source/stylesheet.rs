//! `build_stylesheet` — split out from `page_source.rs`.

use neutron::css::{parse_stylesheet, Stylesheet};
use neutron::dom::{Dom, NodeData, NodeId};

use crate::network::ResourceCache;

use super::url::resolve_url;

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

/// Parses `css_text`, fetches and merges any `@import`s it contains (an
/// import whose trailing media query doesn't match `viewport_width` is
/// skipped without fetching it at all — same "don't do the network work
/// for a rule that couldn't apply anyway" reasoning `<link media="...">`
/// would get if this crate parsed that attribute, which it doesn't yet),
/// then appends its own rules — imported rules land first, same relative
/// order a real `@import` (which must precede other rules) produces. Real
/// `@font-face` rules (`neutron::css::FontFaceRule`) ride along the same way, from
/// both `css_text` itself and anything it `@import`s - `sheet.font_faces`
/// is what `page_source::load_font_faces` (called once, after
/// `build_stylesheet` returns) actually fetches and registers.
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
    cache: &mut ResourceCache,
) {
    let parsed = parse_stylesheet(css_text);
    for import in &parsed.imports {
        let should_fetch = import
            .media
            .as_ref()
            // `DEFAULT_VIEWPORT_HEIGHT` placeholder — see the same note on
            // `Page::layout`'s own call into `build_box_tree_with_viewport`
            // in `page.rs`; this function isn't threaded a real height
            // either today.
            .map(|m| m.matches(viewport_width, neutron::layout::DEFAULT_VIEWPORT_HEIGHT))
            .unwrap_or(true);
        if !should_fetch {
            continue;
        }
        if let Some(resolved) = resolve_url(base_url, &import.url) {
            if let Ok(response) = cache.fetch_cached(&resolved, storage_root, proxy, dns_server) {
                let imported_text = String::from_utf8_lossy(&response.body).into_owned();
                let imported = parse_stylesheet(&imported_text);
                sheet.rules.extend(imported.rules);
                sheet.font_faces.extend(imported.font_faces);
            }
        }
    }
    sheet.rules.extend(parsed.rules);
    sheet.font_faces.extend(parsed.font_faces);
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
    cache: &mut ResourceCache,
) -> Stylesheet {
    let mut sources = Vec::new();
    collect_css_sources(dom, root, &mut sources);

    let mut sheet = parse_stylesheet(BASE_STYLESHEET_SRC);
    for source in sources {
        let css_text = match source {
            CssSource::Inline(text) => Some(text),
            CssSource::Link(href) => resolve_url(base_url, &href)
                .and_then(|url| {
                    cache
                        .fetch_cached(&url, storage_root, proxy, dns_server)
                        .ok()
                })
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
                cache,
            );
        }
    }
    sheet
}
