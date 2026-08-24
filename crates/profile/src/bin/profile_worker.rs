//! The per-profile child process `profile::Profile` spawns. Hosts the
//! html+dom+css+js-runtime+layout-engine+render stack for one profile and
//! publishes rendered frames over `ipc::FrameWriter` on a real per-frame
//! loop, not just in response to `RELOAD` — a fixed ~60 Hz cadence driven
//! by `std::time::Instant`, the same "vsync loop" role a real browser's
//! per-tab process runs. Every tick pumps `js_runtime::Context`'s timers
//! (`setTimeout`/`setInterval`/`requestAnimationFrame` — see
//! `js-runtime`'s `timers` module, which needed exactly this kind of
//! caller to become more than a manually-pumped test fixture) and
//! re-renders from whatever the DOM looks like now, so a `setInterval`
//! callback mutating `textContent` is visibly reflected in the next
//! published frame.
//!
//! `NAVIGATE <url>` now does a real fetch (`net::get`) and renders the
//! actual response — this is no longer just a fixed demo string. A
//! navigated page's own `<style>` blocks and `<link rel="stylesheet"
//! href="...">` sheets are extracted and applied too (`collect_css_sources`
//! walks the parsed DOM in document order; each `<link>`'s href is
//! fetched with a second real `net::get`), cascaded on top of the one
//! hardcoded base stylesheet that used to be all any page ever got — a
//! real page now actually looks like something, not just unstyled text.
//! `<link>` hrefs are resolved against the page's own URL via the real
//! `url` crate (`resolve_url` — relative, root-relative, and
//! scheme-relative hrefs all work now, not just already-absolute ones),
//! then fetched only if the result is `http(s)://` (skips `data:` etc.).
//! `@import` and `@media` are real now too (`css`'s own parser handles
//! both - see that crate for the exact scope): `merge_stylesheet_text`
//! fetches an imported stylesheet's URL (one level deep, media-gated) the
//! same way a `<link>` is fetched, and every page renders against its own
//! real viewport width (`build_box_tree_with_viewport`), so `@media
//! (min-width: ...)` actually reflects the frame this worker is publishing
//! at, not a guess. Remaining scope cuts: every stylesheet fetch is
//! sequential and blocking (same "the fetch blocks
//! this process's render loop for its duration" trade-off `NAVIGATE`
//! itself already makes — a real browser fetches off the render thread
//! and shows a loading state meanwhile; this one just misses a few frame
//! ticks, acceptable since nothing animates during a fetch anyway).
//!
//! Real `<img>` painting now too (`load_images`/`collect_image_sources`,
//! called from `Page::load` the same way `build_stylesheet` fetches
//! `<link>`s): each `<img src>` is resolved against the page's own URL,
//! fetched via `fetch_with_cookies`, and decoded via `image_decode`. The
//! decode/sizing/painting primitives themselves (`image_decode`,
//! `layout_engine::apply_image_sizes`, `render::build_image_list`/
//! `composite_images`) already existed and were crate-tested — this
//! worker just never called any of them, so a real navigated page's
//! `<img>` rendered as an empty box regardless. `Page::render`/
//! `hit_test_at` now share one `Page::layout` helper (previously each
//! rebuilt its own box tree independently) so both agree on the same
//! image-sized boxes.
//!
//! Command protocol over stdin (newline-delimited, one command per line),
//! read on a dedicated thread and drained non-blockingly by the render
//! loop each tick (so a slow/absent command stream never stalls
//! rendering, and a burst of frames never stalls a command response by
//! more than one tick, ~16ms):
//! - `PING` -> replies `PONG` on stdout (liveness check)
//! - `RELOAD` -> fires a real, cancelable `beforeunload` on the current
//!   page's `window` first (see `Context::fire_before_unload`); a listener
//!   calling `preventDefault()` cancels the whole reload, replying
//!   `ERROR navigation canceled by beforeunload` with the old page left
//!   running untouched. Otherwise re-fetches/re-renders whatever page is
//!   currently loaded (the built-in demo page, or the last `NAVIGATE`d
//!   URL — including retrying one that previously failed), creates a
//!   fresh JS context (resetting any JS state - matches a real
//!   navigation), fires real `DOMContentLoaded`/`load` on it once its
//!   scripts have run; replies `RELOADED` once that's done, or
//!   `ERROR <message>` if the (re-)fetch failed, same as `NAVIGATE`
//! - `NAVIGATE <url>` -> same `beforeunload` gate as `RELOAD`, then fetches
//!   `url` and renders it as the new page; replies `NAVIGATED` on success
//!   or `ERROR <message>` on failure (bad URL, network error, ...) —
//!   either way an in-page error message is rendered too, not just
//!   reported over the protocol, so the frame itself never silently goes
//!   stale
//! - `CLICK <#id>` -> dispatches a real `"click"` event (via the existing
//!   `Node.prototype.dispatchEvent` JS binding) at the element with that
//!   id; replies `CLICKED` or `ERROR <message>` (no such id, or only
//!   `#id` selectors are supported at all — this engine has no general
//!   CSS selector query beyond `dom::Dom::find_by_id`)
//! - `FILL <#id> <value>` -> sets that element's real `.value`
//!   (`dom::Dom::value`/`set_value`, independent of children/text) if it's
//!   an `<input>`/`<textarea>`, `textContent` otherwise (this engine's
//!   only settable string for any other element); replies
//!   `FILLED`/`ERROR <message>`. `value` may not contain a newline (this
//!   protocol is newline-delimited) - `profile::Profile::fill` rejects
//!   that before it would corrupt the stream.
//! - `SCROLL <dy>` -> real viewport scroll: shifts the page's scroll
//!   offset by `dy` pixels (positive = down), clamped to
//!   `[0, content_height - viewport_height]` (`0` if the page is shorter
//!   than the viewport) - see `Page::render`/`hit_test_at`'s own docs for
//!   how painting/hit-testing account for it. Replies `SCROLLED`, or
//!   `ERROR <message>` if `dy` doesn't parse as a number. Reset to `0` by
//!   `RELOAD`/`NAVIGATE`, same as `focused_id`. No horizontal scroll.
//! - `QUIT` -> exits cleanly
//! - anything else -> ignored
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use css::{parse_stylesheet, Stylesheet};
use dom::{Dom, NodeData, NodeId};
use image_decode::DecodedImage;
use js_runtime::{Context, Runtime};
use js_runtime::Rect as LayoutMeasurementRect;
use layout_engine::{apply_image_sizes, build_box_tree_with_viewport, layout_block, Color, Display, Length, LayoutBox, Position, PositionedGlyph};
use render::{
    build_display_list, build_glyph_list, build_image_list, composite_glyphs, composite_images, ClipRect, ClippedGlyph, GpuRenderer,
    ImageQuad, Rect,
};

const TARGET_FPS: u32 = 60;
const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / TARGET_FPS as u64);

const DEMO_HTML: &str = r#"
<div id="container">
  <p>Nimble profile worker</p>
  <p>Rendering real HTML via html5ever.</p>
  <p id="counter">tick 0</p>
</div>
"#;

/// Runs only against the built-in demo page (never a real navigated
/// page, which has no `#counter` element for it to find): increments a
/// counter and writes it into `#counter`'s text every 50ms via
/// `setInterval`, and separately bumps a `requestAnimationFrame`-driven
/// counter every tick, proving both timer kinds are actually pumped by
/// the host loop rather than just accepted and ignored.
/// Also demonstrates real persisted `localStorage`/`document.cookie` on
/// the demo page's own `#counter` text: `visits` increments in real
/// `localStorage` every time this script runs (i.e. every `RELOAD`), and
/// `document.cookie` gets a real cookie set - both readable proof (via
/// the rendered text a `profile` test can read back from actual painted
/// pixels) that `Context::with_storage`'s wiring reaches a real page, not
/// just something `js-runtime`'s own unit tests exercise in isolation.
const DEMO_SCRIPT: &str = r#"
var tickCount = 0;
var rafCount = 0;
var visits = parseInt(localStorage.getItem('visits') || '0', 10) + 1;
localStorage.setItem('visits', String(visits));
document.cookie = 'visited=true';
function onTick() {
    tickCount++;
    document.getElementById('counter').textContent = 'tick ' + tickCount + ' raf ' + rafCount + ' visits ' + visits;
    setTimeout(onTick, 50);
}
function onFrame() {
    rafCount++;
    requestAnimationFrame(onFrame);
}
setTimeout(onTick, 50);
requestAnimationFrame(onFrame);
"#;

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
    if let NodeData::Element { tag, attributes, .. } = &n.data {
        if tag == "style" {
            out.push(CssSource::Inline(dom.text_content(node)));
        } else if tag == "link" {
            let is_stylesheet = attributes.get("rel").map(|r| r.eq_ignore_ascii_case("stylesheet")).unwrap_or(false);
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
/// Flattens a laid-out `LayoutBox` tree into a `NodeId -> Rect` map for
/// `Context::set_layout_rects` — the real backing data for
/// `getBoundingClientRect`/`offsetWidth`/etc (see `js_runtime::
/// layout_measurement`). `LayoutBox::dimensions` is already absolute
/// (viewport-relative, unscrolled) document space, the same space
/// `render`'s own `build_display_list` paints from — no coordinate
/// conversion needed, just a straight copy per box. A synthetic inline-run
/// box (`LayoutBox::inline_spans`, see `layout_engine::tree`'s module doc)
/// still has a real `node` id (its first source child's) and dimensions,
/// so it's included like any other box, not skipped.
fn collect_layout_rects(tree: &LayoutBox) -> HashMap<NodeId, LayoutMeasurementRect> {
    fn walk(box_: &LayoutBox, out: &mut HashMap<NodeId, LayoutMeasurementRect>) {
        out.insert(
            box_.node,
            LayoutMeasurementRect {
                x: box_.dimensions.x,
                y: box_.dimensions.y,
                width: box_.dimensions.width,
                height: box_.dimensions.height,
            },
        );
        for child in &box_.children {
            walk(child, out);
        }
    }
    let mut out = HashMap::new();
    walk(tree, &mut out);
    out
}

/// Formats one `layout_engine::Length` the way real CSS would serialize a
/// resolved value: `px` for a resolved pixel length, `auto` for `Auto`, and
/// a plain percent string for `Percent` (real `getComputedStyle` resolves
/// percentages to pixels too, but this crate's `ComputedStyle` doesn't
/// carry the containing-block context needed to do that here — a
/// documented simplification, same shape as `layout-engine`'s own
/// `top`/`bottom` percentage scope cut).
fn format_length(length: Length) -> String {
    match length {
        Length::Px(px) => format!("{px}px"),
        Length::Percent(pct) => format!("{pct}%"),
        Length::Auto => "auto".to_string(),
    }
}

fn format_color(color: Color) -> String {
    format!("rgba({}, {}, {}, {})", color.r, color.g, color.b, color.a as f64 / 255.0)
}

/// Flattens a laid-out `LayoutBox` tree's per-box `ComputedStyle` into a
/// `NodeId -> { property: value }` map for `Context::set_computed_styles`
/// — the real backing data for `getComputedStyle` (see `js_runtime::
/// computed_style`). Only the small, unambiguous-to-serialize property
/// subset below is included; `layout-engine`'s own `ComputedStyle` has
/// several more fields (`flex_grow`, `box_shadow`, ...) not attempted here
/// since a real caller almost never reads those through `getComputedStyle`
/// and this keeps the string-formatting surface bounded.
fn collect_computed_styles(tree: &LayoutBox) -> HashMap<NodeId, HashMap<String, String>> {
    fn walk(box_: &LayoutBox, out: &mut HashMap<NodeId, HashMap<String, String>>) {
        let style = &box_.style;
        let mut properties = HashMap::new();
        properties.insert(
            "display".to_string(),
            match style.display {
                Display::Block => "block",
                Display::Inline => "inline",
                Display::Flex => "flex",
                Display::None => "none",
            }
            .to_string(),
        );
        properties.insert("position".to_string(), match style.position {
            Position::Static => "static",
            Position::Relative => "relative",
            Position::Absolute => "absolute",
        }.to_string());
        properties.insert("width".to_string(), format_length(style.width));
        properties.insert("height".to_string(), format_length(style.height));
        properties.insert("margin-top".to_string(), format_length(style.margin.top));
        properties.insert("margin-right".to_string(), format_length(style.margin.right));
        properties.insert("margin-bottom".to_string(), format_length(style.margin.bottom));
        properties.insert("margin-left".to_string(), format_length(style.margin.left));
        properties.insert("padding-top".to_string(), format_length(style.padding.top));
        properties.insert("padding-right".to_string(), format_length(style.padding.right));
        properties.insert("padding-bottom".to_string(), format_length(style.padding.bottom));
        properties.insert("padding-left".to_string(), format_length(style.padding.left));
        properties.insert("color".to_string(), format_color(style.color));
        properties.insert("background-color".to_string(), format_color(style.background_color));
        properties.insert("font-size".to_string(), format!("{}px", style.font_size));
        properties.insert("opacity".to_string(), style.opacity.to_string());
        out.insert(box_.node, properties);
        for child in &box_.children {
            walk(child, out);
        }
    }
    let mut out = HashMap::new();
    walk(tree, &mut out);
    out
}

fn collect_script_sources(dom: &Dom, node: NodeId, out: &mut Vec<String>) {
    let Some(n) = dom.get(node) else { return };
    if let NodeData::Element { tag, attributes, .. } = &n.data {
        if tag == "script" && !attributes.contains_key("src") {
            out.push(dom.text_content(node));
        }
    }
    for &child in &n.children {
        collect_script_sources(dom, child, out);
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
    if let NodeData::Element { tag, attributes, .. } = &n.data {
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
fn load_images(
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
        let Some(url) = resolve_url(base_url, &src) else { continue };
        let Ok(response) = fetch_with_cookies(&url, storage_root, proxy, dns_server) else { continue };
        let Some(decoded) = image_decode::decode(&response.body) else { continue };
        images.insert(node, Rc::new(decoded));
    }
    images
}

/// Fetches `url` with a `Cookie` header built from the jar at
/// `storage_root/<host>/cookies.txt` (the same file
/// `js_runtime::Context::with_storage` opens for `document.cookie` — see
/// that constructor's doc), then feeds every `Set-Cookie` response header
/// back into that same jar before returning. Because this runs *before*
/// `Page::load` opens its own `Context`, a `Set-Cookie` on the page's own
/// HTML response is already on disk by the time `document.cookie` reads
/// it — real browsers show the same same-navigation consistency. A jar
/// open/write failure (permissions, full disk) degrades to "send/store no
/// cookies" rather than failing the fetch — matches this file's existing
/// storage-failure fallback in `Page::load`.
///
/// `proxy`, when `Some`, routes the request through `net::get_via_proxy`
/// instead of connecting directly — every fetch this worker makes
/// (page HTML, `<link>` stylesheets, `@import`s) shares this one function,
/// so a profile's proxy choice applies to all of them uniformly, not just
/// the page's own HTML. `dns_server`, when `Some` and `proxy` is `None`,
/// resolves `url`'s host through that server instead of the OS resolver
/// via `net::get_via_dns` — closes the "DNS" half of the spec's Settings/
/// Network gap the same way `proxy` closed the proxy half. A proxy always
/// wins if both are set (there's no `net` entry point combining custom
/// DNS resolution with proxy tunneling — a proxied request's DNS
/// resolution is the proxy's own job, not this worker's).
fn fetch_with_cookies(url: &str, storage_root: &std::path::Path, proxy: Option<&net::ProxyConfig>, dns_server: Option<std::net::SocketAddr>) -> Result<net::Response, net::Error> {
    let parsed = url::Url::parse(url).ok();
    let host = parsed.as_ref().and_then(|u| u.host_str()).map(str::to_string);
    let path = parsed.as_ref().map(|u| u.path().to_string()).unwrap_or_else(|| "/".to_string());
    let secure = parsed.as_ref().map(|u| u.scheme() == "https").unwrap_or(false);

    let mut jar = host
        .as_deref()
        .and_then(|h| storage::cookies::CookieJar::open(storage_root.join(h).join("cookies.txt")).ok());

    let cookie_header = jar.as_ref().zip(host.as_deref()).and_then(|(jar, h)| jar.header_value(h, &path, secure));
    let extra_headers: Vec<(&str, &str)> = cookie_header.as_deref().map(|v| vec![("Cookie", v)]).unwrap_or_default();

    let response = match (proxy, dns_server) {
        (Some(proxy), _) => net::get_via_proxy(url, &extra_headers, proxy)?,
        (None, Some(dns_server)) => net::get_via_dns(url, &extra_headers, dns_server)?,
        (None, None) => net::get_with_headers(url, &extra_headers)?,
    };

    if let (Some(jar), Some(host)) = (jar.as_mut(), host.as_deref()) {
        for (name, value) in &response.headers {
            if name.eq_ignore_ascii_case("set-cookie") {
                let _ = jar.set_from_header(value, host);
            }
        }
    }

    Ok(response)
}

/// Parses this worker's optional 5th CLI argument into a
/// [`net::ProxyConfig`]: `"host:port"` (no auth) or
/// `"user:pass@host:port"` (HTTP Basic `Proxy-Authorization`) — the
/// authority portion of a `user:pass@host:port` proxy URL, minus the
/// scheme (this worker only ever tunnels via `CONNECT`, so a `http://`
/// vs `https://` proxy-facing scheme wouldn't change anything). Returns
/// `None` for a missing argument, an empty string, or one that doesn't
/// parse as `host:port` — an unparseable proxy argument degrades to "no
/// proxy" rather than refusing to start, matching this file's general
/// "best-effort, never fails the whole page load over one bad input"
/// stance elsewhere (see `resolve_url`).
fn parse_proxy_arg(arg: Option<&str>) -> Option<net::ProxyConfig> {
    let arg = arg?;
    if arg.is_empty() {
        return None;
    }
    let (credentials, host_port) = match arg.split_once('@') {
        Some((credentials, host_port)) => (Some(credentials), host_port),
        None => (None, arg),
    };
    let (host, port) = host_port.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    let (username, password) = match credentials.and_then(|c| c.split_once(':')) {
        Some((user, pass)) => (Some(user.to_string()), Some(pass.to_string())),
        None => (None, None),
    };
    Some(net::ProxyConfig { host: host.to_string(), port, username, password })
}

/// Parses this worker's optional 6th CLI argument (`"host:port"`) into
/// the DNS server every fetch should resolve through instead of the OS
/// resolver, via `net::get_via_dns`. Same "degrade to none rather than
/// refuse to start" stance as [`parse_proxy_arg`] — a missing argument,
/// empty string, or one that doesn't resolve to a real `SocketAddr`
/// (`host:port` needs the host to already be an IP literal - DNS-syntax
/// resolution of the server's own address isn't attempted, this worker
/// has no resolver to bootstrap one with) all degrade to "use the OS
/// resolver".
fn parse_dns_arg(arg: Option<&str>) -> Option<std::net::SocketAddr> {
    let arg = arg?;
    if arg.is_empty() {
        return None;
    }
    arg.parse().ok()
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

/// Resolves every CSS source in `dom` (rooted at `root`) into one cascaded
/// [`Stylesheet`]: [`BASE_STYLESHEET_SRC`] first, then each `<style>`/
/// `<link>` source in document order, `<link>` hrefs resolved against
/// `base_url` via [`resolve_url`] — a `<link>` that doesn't
/// resolve to a fetchable `http(s)://` URL is silently skipped, matching
/// `resolve_html`'s own "best-effort, never panics on a real page" stance
/// rather than failing the whole page load over one bad stylesheet
/// reference.
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
        let should_fetch = import.media.as_ref().map(|m| m.matches(viewport_width)).unwrap_or(true);
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

fn build_stylesheet(
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
            merge_stylesheet_text(&mut sheet, &css_text, base_url, viewport_width, storage_root, proxy, dns_server);
        }
    }
    sheet
}

/// What the currently loaded page came from - re-resolved on `RELOAD`.
enum PageSource {
    Demo,
    Url(String),
}

fn escape_html_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn error_page_html(url: &str, message: &str) -> String {
    format!(
        r#"<div id="container"><p>Failed to load {}</p><p>{}</p></div>"#,
        escape_html_text(url),
        escape_html_text(message)
    )
}

/// Resolves `source` to real HTML: the built-in demo string, or a real
/// `net::get` response body decoded as UTF-8 (lossily - this doesn't
/// attempt charset detection from `Content-Type`/`<meta charset>`, real
/// HTML often isn't UTF-8 but plenty is, and this crate doesn't have a
/// non-UTF-8 text decoder yet).
fn resolve_html(source: &PageSource, storage_root: &std::path::Path, proxy: Option<&net::ProxyConfig>, dns_server: Option<std::net::SocketAddr>) -> Result<String, String> {
    match source {
        PageSource::Demo => Ok(DEMO_HTML.to_string()),
        PageSource::Url(url) => fetch_with_cookies(url, storage_root, proxy, dns_server)
            .map(|response| String::from_utf8_lossy(&response.body).into_owned())
            .map_err(|e| e.to_string()),
    }
}

/// Only `#id` selectors are supported for `CLICK`/`FILL` — this engine has
/// no CSS selector query beyond `dom::Dom::find_by_id` (no `css`-backed
/// `querySelector`, no class/attribute/descendant matching against a live
/// `Dom`). Rejects any other form up front rather than silently matching
/// nothing.
fn require_id_selector(selector: &str) -> Result<&str, String> {
    selector
        .strip_prefix('#')
        .filter(|id| !id.is_empty())
        .ok_or_else(|| format!("unsupported selector \"{selector}\" - only #id selectors are implemented"))
}

/// A valid JS double-quoted string literal for `s` - just enough escaping
/// to safely embed an arbitrary Rust string (an id, an error message, a
/// fill value) into the small JS snippets `dispatch_click`/`fill_element`
/// build and eval, without a real `JS_NewStringLen`-based binding for
/// "call this method with this string argument" existing yet (the only
/// public entry point into a `Context` is `eval(source)` - see that
/// method's own doc).
fn js_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Dispatches a real `"click"` event at the `#id` element via the
/// already-real `Node.prototype.dispatchEvent` JS binding - if the page's
/// own script attached a `"click"` listener via `addEventListener`, it
/// actually runs. `Err` covers both "no such id" and the listener itself
/// throwing - `js_runtime::Context::eval`'s own placeholder exception
/// message (see its doc comment) can't currently distinguish the two, so
/// this reports the more actionable one.
fn dispatch_click(ctx: &Context, selector: &str) -> Result<(), String> {
    let id = require_id_selector(selector)?;
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.dispatchEvent(\"click\"); }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
    );
    ctx.eval(&script, "<pane click>").map(|_| ()).map_err(|_| format!("no element with id \"{id}\" (or its click handler threw)"))
}

/// Real focus, via the JS `.focus()` binding rather than calling
/// `dom::Dom::focus` directly — routing through JS means the real
/// `"focus"` event dispatches too (`dom_bindings::node_focus` calls
/// `events::dispatch` after mutating `dom::Dom`'s focus state), same
/// reasoning `dispatch_click` already applies to clicks.
fn focus_element(ctx: &Context, id: &str) -> Result<(), String> {
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.focus(); }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
    );
    ctx.eval(&script, "<pane focus>").map(|_| ()).map_err(|_| format!("no element with id \"{id}\""))
}

/// Real blur, via the JS `.blur()` binding — see `focus_element`'s own doc
/// for why this goes through JS instead of `dom::Dom::blur` directly:
/// `dom_bindings::node_blur` dispatches a real `"blur"` event, and a real
/// `"change"` event too if `.value` moved since the matching `focus()`.
fn blur_element(ctx: &Context, id: &str) -> Result<(), String> {
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.blur(); }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
    );
    ctx.eval(&script, "<pane blur>").map(|_| ()).map_err(|_| format!("no element with id \"{id}\""))
}

/// `true` if `node` is a real `<input>`/`<textarea>` — the only tags this
/// engine gives an independent `.value` (see `dom::Dom::value`'s own doc
/// on why that's exposed generically on `Node` rather than a typed
/// `HTMLInputElement`/`HTMLTextAreaElement` subclass).
fn is_input_like(dom: &Dom, node: NodeId) -> bool {
    matches!(&dom.get(node).map(|n| &n.data), Some(NodeData::Element { tag, .. }) if tag == "input" || tag == "textarea")
}

/// Sets the `#id` element's `.value` (a real, independent `dom::Dom`
/// property now — see `is_input_like`/`dom::Dom::value`) if it's an
/// `<input>`/`<textarea>`, `textContent` otherwise (this engine's only
/// settable string for a generic element).
fn fill_element(ctx: &Context, selector: &str, value: &str) -> Result<(), String> {
    let id = require_id_selector(selector)?;
    let prop = {
        let dom_ref = ctx.dom().ok_or("no DOM available")?;
        let node = dom_ref.find_by_id(id).ok_or_else(|| format!("no element with id \"{id}\""))?;
        if is_input_like(dom_ref, node) { "value" } else { "textContent" }
    };
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.{prop} = {value}; }})();",
        id = js_string_literal(id),
        missing = js_string_literal(&format!("no element with id \"{id}\"")),
        value = js_string_literal(value),
    );
    ctx.eval(&script, "<pane fill>").map(|_| ()).map_err(|_| format!("no element with id \"{id}\""))
}

/// One (re)loadable "page": the parsed DOM's root `<html>` element, its
/// own resolved stylesheet (base + whatever `<style>`/`<link>` sources it
/// contributed), and the JS context mutating it. Rebuilt from scratch on
/// every navigation/reload, same as a real navigation resetting a page's
/// JS state (and its stylesheet — a page's CSS doesn't survive its own
/// reload any more than its JS does, matching real navigation).
struct Page<'rt> {
    ctx: Context<'rt>,
    html_el: NodeId,
    sheet: Stylesheet,
    /// Real fetched/decoded `<img>` images, keyed by the `<img>` element's
    /// own `NodeId` — see `load_images`. Fixed for this `Page`'s lifetime
    /// (fetched once at load time, same "a page's resources don't change
    /// underneath it without a fresh navigation" convention its
    /// stylesheet already follows).
    images: HashMap<NodeId, Rc<DecodedImage>>,
    /// Real incremental layout invalidation: the last computed tree, plus
    /// the exact inputs it was computed from. `layout()` reuses it
    /// (a clone — far cheaper than re-running `build_box_tree_with_viewport`
    /// + text shaping/flex resolution + `layout_block` from scratch) when
    /// none of `width`/`dom.mutation_count()`/`adopted_stylesheet_text()`
    /// have changed since. `RefCell` because `layout()` is called from
    /// both `&self` methods (`content_height`, `hit_test_at`) and `&mut
    /// self` ones (`render`) — a shared cache needs interior mutability
    /// either way. `None` before the first `layout()` call.
    layout_cache: std::cell::RefCell<Option<LayoutCache>>,
}

struct LayoutCache {
    width: u32,
    dom_mutations: u64,
    adopted_text: String,
    tree: LayoutBox,
}

impl<'rt> Page<'rt> {
    /// Builds a page from `html`. `run_demo_script` should only be `true`
    /// for the built-in demo page - a real fetched page has no
    /// `#counter` element for `DEMO_SCRIPT` to find (which now uses real
    /// `localStorage`/`document.cookie` itself - see the const's doc).
    /// `document.cookie`/`localStorage`/`sessionStorage`/`indexedDB` are
    /// wired to real per-`storage_host` storage under `storage_root` (via
    /// `js_runtime::Context::with_storage`) whenever `storage_host` is
    /// `Some` - `None` gets a plain `Context::with_dom` instead. A
    /// storage-open failure (rare - a permissions problem, a full disk)
    /// degrades to `with_dom` rather than failing the whole page load.
    fn load(
        runtime: &'rt Runtime,
        html: &str,
        run_demo_script: bool,
        base_url: Option<&str>,
        storage_host: Option<&str>,
        viewport_width: f64,
        storage_root: &std::path::Path,
        proxy: Option<&net::ProxyConfig>,
        dns_server: Option<std::net::SocketAddr>,
    ) -> Self {
        let (dom, html_el) = html::parse_to_html_element(html);
        let sheet = build_stylesheet(&dom, html_el, base_url, viewport_width, storage_root, proxy, dns_server);
        let images = load_images(&dom, html_el, base_url, storage_root, proxy, dns_server);
        let mut scripts = Vec::new();
        collect_script_sources(&dom, html_el, &mut scripts);

        let mut ctx = match storage_host {
            Some(host) => match Context::with_storage(runtime, dom, host, storage_root.join(host)) {
                Ok(ctx) => ctx,
                // `with_storage` already consumed `dom` by the time it can
                // fail (a rare I/O error opening the storage files) - it
                // has to be re-parsed from `html` rather than reused,
                // acceptable for a path this unlikely to hit in practice.
                Err(_) => Context::with_dom(runtime, html::parse_to_html_element(html).0),
            },
            None => Context::with_dom(runtime, dom),
        };
        if let Some(url) = base_url {
            ctx.set_url(url);
        }
        if run_demo_script {
            let _ = ctx.eval(DEMO_SCRIPT, "<profile-worker demo>");
        }
        // Real page scripts, in real document order - a later script
        // failing (a real JS exception) doesn't stop earlier ones from
        // having already run, matching how a real browser keeps executing
        // a page after one `<script>` throws (each gets its own top-level
        // try - this engine still has no `window.onerror`/console to
        // report it to, so a failure is silent here, same "no error
        // surface for a page's own script" scope every other page-script
        // path in this worker already has).
        for (index, script) in scripts.iter().enumerate() {
            let _ = ctx.eval(script, &format!("<script {index}>"));
        }
        // Real `DOMContentLoaded`/`load` lifecycle timing: fired once the
        // document is parsed and every page script has run, matching a
        // real browser's own ordering (see `Context::dispatch_lifecycle_events`'s
        // doc for the scope this crate cuts relative to the full spec).
        ctx.dispatch_lifecycle_events();
        Page { ctx, html_el, sheet, images, layout_cache: std::cell::RefCell::new(None) }
    }

    /// Builds and lays out this page's real box tree against `width` —
    /// shared by `render` and `hit_test_at` so they always agree on
    /// exactly the same box positions/sizes (previously each rebuilt its
    /// own tree independently, which would have silently disagreed once
    /// `<img>` intrinsic sizing entered the picture - `apply_image_sizes`
    /// must run after box-tree construction and before layout, so both
    /// callers need it applied identically). `None` only if the parse/
    /// box-tree-construction step itself fails.
    fn layout(&self, width: u32) -> Option<LayoutBox> {
        let dom = self.ctx.dom()?;
        let dom_mutations = dom.mutation_count();
        // Real `document.adoptedStyleSheets` mutation support (see
        // `js_runtime::cssom_stylesheet`): re-read every adopted sheet's
        // rules so a script's `insertRule`/`deleteRule` (which don't bump
        // `dom.mutation_count()` - they touch a `CSSStyleSheet`, not the
        // DOM) still invalidates the cache below.
        let adopted_text = self.ctx.adopted_stylesheet_text();

        if let Some(cached) = self.layout_cache.borrow().as_ref() {
            if cached.width == width && cached.dom_mutations == dom_mutations && cached.adopted_text == adopted_text {
                return Some(cached.tree.clone());
            }
        }

        // Merges the page's own base stylesheet (built once at `load()`
        // time) with whatever's currently adopted - cloning rather than
        // mutating `self.sheet` in place keeps the base untouched if a
        // later layout has nothing adopted anymore.
        let sheet = if adopted_text.is_empty() {
            std::borrow::Cow::Borrowed(&self.sheet)
        } else {
            let mut merged = self.sheet.clone();
            merged.rules.extend(parse_stylesheet(&adopted_text).rules);
            std::borrow::Cow::Owned(merged)
        };
        let mut tree = build_box_tree_with_viewport(dom, self.html_el, &sheet, width as f64)?;
        apply_image_sizes(dom, &mut tree, &self.images);
        layout_block(&mut tree, width as f64, 0.0, 0.0);

        *self.layout_cache.borrow_mut() = Some(LayoutCache { width, dom_mutations, adopted_text, tree: tree.clone() });
        Some(tree)
    }

    /// This page's real total content height at `width` — the root box's
    /// own laid-out height, which already includes every descendant
    /// (block layout stacks children downward with no clamping to any
    /// viewport). Used to clamp `SCROLL`'s offset to real content, not an
    /// arbitrary range. `0.0` if layout itself fails (same "nothing to
    /// scroll" outcome as a page shorter than its viewport).
    fn content_height(&self, width: u32) -> f64 {
        self.layout(width).map(|tree| tree.dimensions.height).unwrap_or(0.0)
    }

    /// Re-layouts and re-rasterizes from the DOM's *current* state (which
    /// may have just been mutated by a JS timer callback) against an
    /// already-initialized `renderer` — creating a `GpuRenderer` opens a
    /// real GPU device/adapter, expensive enough that doing it every tick
    /// would itself become the vsync loop's bottleneck instead of the
    /// fixed frame interval. Composites real decoded `<img>` pixels
    /// (`self.images`, via `build_image_list`/`composite_images`) between
    /// the background-rect pass and the text pass — closest to real paint
    /// order (background, then replaced content, then inline text) this
    /// worker's existing two-pass (GPU rects, then CPU glyphs) pipeline
    /// can give without a bigger repaint-ordering rework.
    ///
    /// `scroll_top` is a real viewport scroll offset (see the `SCROLL`
    /// command's own doc): every painted rect/image/glyph is shifted up
    /// by this many pixels before compositing, so what's actually visible
    /// in `[0, height)` is the document's `[scroll_top, scroll_top +
    /// height)` slice — layout itself is unaffected (boxes keep their
    /// real absolute document-space positions; only the paint step
    /// windows into them), which is also why `build_display_list`/
    /// `build_image_list`/`build_glyph_list` don't need a `scroll_top`
    /// parameter of their own. `render`/`gpu`/`text`'s own compositors
    /// already clip anything outside `[0, height)` silently (real
    /// clipping, not new for this), so a shifted rect above or below the
    /// viewport is simply not drawn - no separate clip step needed here.
    /// Real `overflow: hidden`/`auto`/`scroll` clipping (per-element, see
    /// `layout_engine::Overflow`) rides along the same shift: an
    /// `ImageQuad`/`ClippedGlyph`'s own `clip` region is in the same
    /// absolute document space as everything else, so it needs the same
    /// `-offset`/`-scroll_top` shift applied as the quad/glyph it clips,
    /// or a scrolled page would clip against a stale, unscrolled region.
    fn render(&mut self, renderer: &GpuRenderer, width: u32, height: u32, scroll_top: f64) -> Vec<u8> {
        let tree = self.layout(width).expect("parsed HTML always produces a box");
        self.ctx.set_layout_rects(collect_layout_rects(&tree));
        self.ctx.set_computed_styles(collect_computed_styles(&tree));
        let offset = scroll_top as f32;
        let shift_clip = |clip: Option<ClipRect>, dy: f32| clip.map(|c| ClipRect { y: c.y - dy, ..c });

        let rects: Vec<Rect> = build_display_list(&tree).into_iter().map(|r| Rect { y: r.y - offset, ..r }).collect();
        let images: Vec<ImageQuad> = build_image_list(&tree)
            .into_iter()
            .map(|q| ImageQuad {
                y: q.y - offset,
                clip: shift_clip(q.clip, offset),
                ..q
            })
            .collect();
        let glyphs: Vec<ClippedGlyph> = build_glyph_list(&tree)
            .into_iter()
            .map(|g| ClippedGlyph {
                glyph: PositionedGlyph { y: g.glyph.y - scroll_top as i32, ..g.glyph },
                clip: shift_clip(g.clip, offset),
                opacity: g.opacity,
            })
            .collect();

        let mut pixels = renderer.render_to_rgba(&rects, width, height, [0.08, 0.09, 0.13, 1.0]);
        composite_images(&mut pixels, width, height, &images);
        composite_glyphs(&mut pixels, width, height, &glyphs);
        pixels
    }

    /// Real coordinate-to-DOM-node hit test — lays out the same box tree
    /// `render` does (via `self.layout`, against `width`, the pane's own
    /// real frame width, so the caller's `(x, y)` must already be in that
    /// same on-screen pixel space) and walks it via
    /// `layout_engine::hit_test`. `scroll_top` converts `y` from that
    /// on-screen space back to the document's own absolute space (the
    /// inverse of the shift `render` applies when painting) before
    /// hit-testing, so a click against a scrolled page still lands on the
    /// real element under the cursor, not whatever was there before any
    /// `SCROLL`. `None` covers both "point is outside every box" and "the
    /// parse/layout step itself failed" — a caller can't distinguish
    /// those from this return value alone, matching this method's only
    /// real use (`CLICK_AT`, where both cases report the same "nothing
    /// there" outcome anyway).
    fn hit_test_at(&self, width: u32, x: f64, y: f64, scroll_top: f64) -> Option<NodeId> {
        let tree = self.layout(width)?;
        layout_engine::hit_test(&tree, x, y + scroll_top)
    }
}

/// Walks from `node` up through real `dom::Dom` parent links (not a JS
/// call — this runs before any JS-level dispatch happens) looking for the
/// nearest ancestor (including `node` itself) that has a real `id`
/// attribute. `CLICK_AT`'s real limitation, same as automation's existing
/// `#id`-only `CLICK`/`FILL` (see `require_id_selector`'s own doc): a
/// coordinate click can only ever reach an element the page itself gave
/// an id to, directly or via one of its ancestors — real event bubbling
/// still finds *a* real listener target this way in the common case
/// (buttons/links typically carry an id, or a parent does), but a click
/// on a page with no ids anywhere genuinely can't be dispatched, and this
/// returns `None` rather than silently no-opping past that.
fn nearest_id_ancestor(dom: &Dom, node: NodeId) -> Option<String> {
    let mut current = Some(node);
    while let Some(id) = current {
        if let Some(value) = dom.attribute(id, "id") {
            return Some(value.to_string());
        }
        current = dom.get(id).and_then(|n| n.parent);
    }
    None
}

/// Real coordinate click: hit-tests `(x, y)`, dispatches a real `"click"`
/// via the nearest id-addressable ancestor (see `nearest_id_ancestor`'s
/// own doc for the real limitation that implies), and — real focus model,
/// via `focus_element`/`blur_element` (JS `.focus()`/`.blur()`, not
/// `dom::Dom::focus`/`clear_focus` called directly, so real `"focus"`/
/// `"blur"`/`"change"` events dispatch too) — focuses the hit node itself
/// if it's a real `<input>`/`<textarea>` with its own `id` (so `KEY` can
/// type into it, and `document.activeElement` sees it too), or clears
/// focus otherwise (matches a real browser blurring whatever was focused
/// when a click lands somewhere non-focusable). Whatever was focused
/// before is blurred first, same order a real browser fires them in
/// (`blur` on the old element before `focus` on the new one). A click that
/// lands back on the already-focused element is a no-op for focus (no
/// redundant `blur`+`focus` pair), matching a real browser not re-firing
/// focus on a click to a field that already has it. See `tab_focus` for
/// the other way focus moves - a real `Tab`/`Shift+Tab` press.
fn dispatch_click_at(page: &mut Page, width: u32, x: f64, y: f64, scroll_top: f64) -> Result<Option<String>, String> {
    let node = page.hit_test_at(width, x, y, scroll_top).ok_or("no element at that point")?;
    let click_id = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        nearest_id_ancestor(dom_ref, node).ok_or("no id-addressable element at or above that point")?
    };
    dispatch_click(&page.ctx, &format!("#{click_id}"))?;

    // Re-borrow fresh rather than reusing the pre-dispatch `dom_ref` - the
    // click listener that just ran (real JS, via `dispatch_click` above)
    // could have mutated the DOM, and this engine's arena (`dom::Dom`'s
    // internal `Vec<Slot>`) isn't guaranteed not to reallocate on a node
    // creation in between.
    let (focusable, focus_id, already_focused, previously_focused_id) = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        let focusable = is_input_like(dom_ref, node);
        let focus_id = if focusable { dom_ref.attribute(node, "id").map(str::to_string) } else { None };
        let already_focused = focusable && dom_ref.active_element() == Some(node);
        let previously_focused_id = if already_focused {
            None
        } else {
            dom_ref.active_element().and_then(|prev| dom_ref.attribute(prev, "id").map(str::to_string))
        };
        (focusable, focus_id, already_focused, previously_focused_id)
    };
    if let Some(prev_id) = previously_focused_id {
        let _ = blur_element(&page.ctx, &prev_id);
    }
    if focusable && !already_focused {
        if let Some(id) = &focus_id {
            let _ = focus_element(&page.ctx, id);
        }
    }
    Ok(focus_id)
}

/// Types `key` into whichever real `<input>`/`<textarea>` id `CLICK_AT`
/// most recently focused (see `dispatch_click_at`'s doc) - `"Backspace"`
/// removes the field's real last character, anything else is appended
/// verbatim as typed text. Writes the element's real `.value`
/// (`dom::Dom::value`/`set_value`) now, not `textContent` — a real
/// `"keydown"` event still dispatches first via the existing
/// `dispatchEvent` binding, so a page's own `keydown` listener genuinely
/// runs, same as a real browser firing the event before applying the
/// default action.
fn type_key(ctx: &Context, focused_id: &str, key: &str) -> Result<(), String> {
    let current = {
        let dom_ref = ctx.dom().ok_or("no DOM available")?;
        let node = dom_ref.find_by_id(focused_id).ok_or_else(|| format!("no element with id \"{focused_id}\""))?;
        dom_ref.value(node)
    };
    let updated = if key == "Backspace" {
        let mut chars: Vec<char> = current.chars().collect();
        chars.pop();
        chars.into_iter().collect()
    } else {
        format!("{current}{key}")
    };
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.dispatchEvent(\"keydown\"); el.value = {value}; }})();",
        id = js_string_literal(focused_id),
        missing = js_string_literal(&format!("no element with id \"{focused_id}\"")),
        value = js_string_literal(&updated),
    );
    ctx.eval(&script, "<pane key>").map(|_| ()).map_err(|_| format!("no element with id \"{focused_id}\" (or its keydown handler threw)"))
}

/// Real `Tab` (`reverse: false`) / `Shift+Tab` (`reverse: true`) focus
/// movement — computes the next target via `dom::Dom::next_focus_target`'s
/// own cycling rule, then blurs whatever was focused before and focuses
/// the new target through the real JS `.blur()`/`.focus()` bindings
/// (`blur_element`/`focus_element`, same as `dispatch_click_at`), so real
/// `"blur"`/`"change"`/`"focus"` events fire. Real limitation shared with
/// every other id-addressed command in this protocol (`CLICK`/`FILL`/
/// `CLICK_AT`): a tab-order target with no `id` attribute can't be
/// reached through a `document.getElementById`-based `eval()` script, so
/// this walks the tab order from the computed starting point in the same
/// direction (wrapping at most once around the whole list) until it finds
/// one with a real `id`, rather than silently landing on an unreachable
/// target. `Ok(None)` when the tab order is empty or every element in it
/// lacks an `id` - both report as "nothing to tab to" to the caller, same
/// as `hit_test_at`'s "nothing there" convention for `CLICK_AT`.
fn tab_focus(page: &mut Page, reverse: bool) -> Result<Option<String>, String> {
    let (target_id, previously_focused_id) = {
        let dom_ref = page.ctx.dom().ok_or("no DOM available")?;
        let order = dom_ref.tab_order();
        if order.is_empty() {
            return Ok(None);
        }
        let current_index = dom_ref.active_element().and_then(|id| order.iter().position(|&n| n == id));
        let start = match (current_index, reverse) {
            (Some(i), false) => (i + 1) % order.len(),
            (Some(i), true) => (i + order.len() - 1) % order.len(),
            (None, false) => 0,
            (None, true) => order.len() - 1,
        };
        let step: i64 = if reverse { -1 } else { 1 };
        let target_id = (0..order.len()).find_map(|offset| {
            let index = (start as i64 + step * offset as i64).rem_euclid(order.len() as i64) as usize;
            dom_ref.attribute(order[index], "id").map(str::to_string)
        });
        let previously_focused_id = dom_ref.active_element().and_then(|prev| dom_ref.attribute(prev, "id").map(str::to_string));
        (target_id, previously_focused_id)
    };

    let Some(target_id) = target_id else {
        return Ok(None);
    };
    if let Some(prev_id) = previously_focused_id {
        if prev_id != target_id {
            let _ = blur_element(&page.ctx, &prev_id);
        }
    }
    focus_element(&page.ctx, &target_id)?;
    Ok(Some(target_id))
}

/// Loads `source`, returning the built page and `Some(error)` if the
/// underlying fetch failed (the returned page is then the rendered error
/// page, not a panic/empty page - callers still get something to publish
/// either way).
/// The demo page's real storage host - not a URL, so not derived via
/// `url::Url::parse` like a navigated page's host is; a fixed name is
/// enough for it to get real per-origin `localStorage`/`document.cookie`
/// (see `DEMO_SCRIPT`).
const DEMO_STORAGE_HOST: &str = "demo.internal";

fn load_source<'rt>(
    runtime: &'rt Runtime,
    source: &PageSource,
    viewport_width: f64,
    storage_root: &std::path::Path,
    proxy: Option<&net::ProxyConfig>,
    dns_server: Option<std::net::SocketAddr>,
) -> (Page<'rt>, Option<String>) {
    let base_url = match source {
        PageSource::Demo => None,
        PageSource::Url(url) => Some(url.as_str()),
    };
    let storage_host = match source {
        PageSource::Demo => Some(DEMO_STORAGE_HOST.to_string()),
        PageSource::Url(url) => url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)),
    };
    match resolve_html(source, storage_root, proxy, dns_server) {
        Ok(html) => {
            let run_demo_script = matches!(source, PageSource::Demo);
            (Page::load(runtime, &html, run_demo_script, base_url, storage_host.as_deref(), viewport_width, storage_root, proxy, dns_server), None)
        }
        Err(message) => {
            let url = match source {
                PageSource::Demo => "the demo page",
                PageSource::Url(url) => url,
            };
            // The synthetic error page has no `<link>`s of its own to
            // resolve - `base_url: None` here, not the failed `url`. It
            // still gets real storage scoped to the same host though, so
            // a subsequent successful load/reload of that host sees
            // consistent state.
            (
                Page::load(runtime, &error_page_html(url, &message), false, None, storage_host.as_deref(), viewport_width, storage_root, proxy, dns_server),
                Some(message),
            )
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 || args.len() > 7 {
        eprintln!("usage: profile-worker <shmem-name> <width> <height> [proxy: host:port or user:pass@host:port] [dns-server: host:port] [gpu-adapter-index]");
        std::process::exit(2);
    }
    let shmem_name = &args[1];
    let width: u32 = args[2].parse().expect("width must be a positive integer");
    let height: u32 = args[3].parse().expect("height must be a positive integer");
    let proxy = parse_proxy_arg(args.get(4).map(String::as_str));
    let dns_server = parse_dns_arg(args.get(5).map(String::as_str));
    // A missing/empty/unparseable value degrades to `render::GpuRenderer::
    // new`'s own default-adapter heuristic - same "refuse to be strict
    // about an optional trailing arg" stance as `dns_server`, not `proxy`'s
    // stricter one (there's no "adapter selection failed" state worth
    // reporting back over the stdin/stdout protocol; it just falls back).
    let gpu_adapter: Option<usize> = args.get(6).filter(|s| !s.is_empty()).and_then(|s| s.parse().ok());

    // One storage root per worker process, keyed by shmem name (already
    // unique per spawned profile) so two profiles never share cookies/
    // localStorage, then further split by host under `Page::load` -
    // real per-origin partitioning. Doesn't persist across the worker
    // process's own lifetime yet - see `Page::load`'s doc.
    let storage_root = std::env::temp_dir().join("nimble-profile-storage").join(shmem_name);

    let runtime = Runtime::new();
    let mut current_source = PageSource::Demo;
    let (mut page, _) = load_source(&runtime, &current_source, width as f64, &storage_root, proxy.as_ref(), dns_server);
    let renderer = match gpu_adapter {
        Some(index) => GpuRenderer::new_with_adapter(index),
        None => GpuRenderer::new(),
    };

    let mut writer = ipc::FrameWriter::new(shmem_name, width, height).expect("failed to create/open shared memory");
    writer.publish(&page.render(&renderer, width, height, 0.0));

    // Commands arrive on a dedicated thread so a slow/absent stdin stream
    // never blocks the render loop below - `try_recv` drains whatever's
    // arrived so far on every tick instead.
    let (cmd_tx, cmd_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if cmd_tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut stdout = io::stdout();
    // Mutable, runtime-adjustable cap for the mockup's "Settings >
    // Performance > frame cap" knob (see `SET_FPS_CAP` below) - starts at
    // the fixed `FRAME_INTERVAL` this loop always used before that command
    // existed, so a caller that never sends it gets identical behavior.
    let mut frame_interval = FRAME_INTERVAL;
    let mut next_tick = Instant::now() + frame_interval;
    // Real "background throttling" (mockup's Settings > Performance knob):
    // while `true`, the tick below skips JS timer pumping and re-rendering
    // entirely instead of just rendering an unchanged page - a genuinely
    // idle loop, not a disguised zero-work render. `PING`/`SET_FPS_CAP`/
    // `QUIT` still work while paused (a hidden pane should still answer
    // liveness checks and settings changes), only the vsync work itself
    // stops.
    let mut paused = false;
    // Real "which id does the next KEY affect" state - a thin cache of
    // `dom::Dom`'s own real `active_element()` (see `dispatch_click_at`'s
    // doc), kept as a `String` id here since `KEY`'s handler goes through
    // `id`-selector `eval()` scripts like every other command in this
    // protocol. Reset on `RELOAD`/`NAVIGATE` since a fresh `Page` means a
    // fresh DOM the old id might not even exist in anymore (a fresh
    // `dom::Dom` starts with no focus of its own either).
    let mut focused_id: Option<String> = None;
    // Real viewport scroll offset (see `SCROLL`'s own handling below and
    // `Page::render`/`hit_test_at`'s docs) - reset to `0.0` on
    // `RELOAD`/`NAVIGATE` same as `focused_id`, since a fresh page always
    // starts scrolled to the top.
    let mut scroll_top: f64 = 0.0;

    'render_loop: loop {
        while let Ok(line) = cmd_rx.try_recv() {
            let line = line.trim();
            if line == "PING" {
                let _ = writeln!(stdout, "PONG");
                let _ = stdout.flush();
            } else if line == "PAUSE" {
                paused = true;
                let _ = writeln!(stdout, "PAUSED");
                let _ = stdout.flush();
            } else if line == "RESUME" {
                paused = false;
                next_tick = Instant::now() + frame_interval;
                let _ = writeln!(stdout, "RESUMED");
                let _ = stdout.flush();
            } else if line == "RELOAD" {
                if !page.ctx.fire_before_unload() {
                    let _ = writeln!(stdout, "ERROR navigation canceled by beforeunload");
                    let _ = stdout.flush();
                    continue;
                }
                let (loaded, error) = load_source(&runtime, &current_source, width as f64, &storage_root, proxy.as_ref(), dns_server);
                page = loaded;
                focused_id = None;
                scroll_top = 0.0;
                writer.publish(&page.render(&renderer, width, height, scroll_top));
                match error {
                    Some(message) => {
                        let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                    }
                    None => {
                        let _ = writeln!(stdout, "RELOADED");
                    }
                }
                let _ = stdout.flush();
            } else if let Some(url) = line.strip_prefix("NAVIGATE ") {
                if !page.ctx.fire_before_unload() {
                    let _ = writeln!(stdout, "ERROR navigation canceled by beforeunload");
                    let _ = stdout.flush();
                    continue;
                }
                current_source = PageSource::Url(url.trim().to_string());
                let (loaded, error) = load_source(&runtime, &current_source, width as f64, &storage_root, proxy.as_ref(), dns_server);
                page = loaded;
                focused_id = None;
                scroll_top = 0.0;
                writer.publish(&page.render(&renderer, width, height, scroll_top));
                match error {
                    Some(message) => {
                        let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                    }
                    None => {
                        let _ = writeln!(stdout, "NAVIGATED");
                    }
                }
                let _ = stdout.flush();
            } else if let Some(rest) = line.strip_prefix("CLICK ") {
                let result = dispatch_click(&page.ctx, rest.trim());
                writer.publish(&page.render(&renderer, width, height, scroll_top));
                match result {
                    Ok(()) => {
                        let _ = writeln!(stdout, "CLICKED");
                    }
                    Err(message) => {
                        let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                    }
                }
                let _ = stdout.flush();
            } else if let Some(rest) = line.strip_prefix("CLICK_AT ") {
                let mut parts = rest.split_whitespace();
                let coords = parts.next().zip(parts.next()).and_then(|(x, y)| Some((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?)));
                match coords {
                    Some((x, y)) => {
                        let result = dispatch_click_at(&mut page, width, x, y, scroll_top);
                        writer.publish(&page.render(&renderer, width, height, scroll_top));
                        match result {
                            Ok(focus) => {
                                focused_id = focus;
                                let _ = writeln!(stdout, "CLICKED");
                            }
                            Err(message) => {
                                let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                            }
                        }
                    }
                    None => {
                        let _ = writeln!(stdout, "ERROR CLICK_AT requires two numeric coordinates");
                    }
                }
                let _ = stdout.flush();
            } else if let Some(key) = line.strip_prefix("KEY ") {
                match &focused_id {
                    Some(id) => {
                        let result = type_key(&page.ctx, id, key);
                        writer.publish(&page.render(&renderer, width, height, scroll_top));
                        match result {
                            Ok(()) => {
                                let _ = writeln!(stdout, "TYPED");
                            }
                            Err(message) => {
                                let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                            }
                        }
                    }
                    None => {
                        let _ = writeln!(stdout, "ERROR no element focused - CLICK_AT an <input>/<textarea> first");
                    }
                }
                let _ = stdout.flush();
            } else if line == "TAB" || line == "TAB_REVERSE" {
                let reverse = line == "TAB_REVERSE";
                let result = tab_focus(&mut page, reverse);
                writer.publish(&page.render(&renderer, width, height, scroll_top));
                match result {
                    Ok(Some(id)) => {
                        focused_id = Some(id);
                        let _ = writeln!(stdout, "TABBED");
                    }
                    Ok(None) => {
                        let _ = writeln!(stdout, "ERROR no focusable element with an id on this page");
                    }
                    Err(message) => {
                        let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                    }
                }
                let _ = stdout.flush();
            } else if let Some(rest) = line.strip_prefix("FILL ") {
                let mut parts = rest.splitn(2, ' ');
                let selector = parts.next().unwrap_or("").trim();
                let value = parts.next().unwrap_or("");
                let result = fill_element(&page.ctx, selector, value);
                writer.publish(&page.render(&renderer, width, height, scroll_top));
                match result {
                    Ok(()) => {
                        let _ = writeln!(stdout, "FILLED");
                    }
                    Err(message) => {
                        let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                    }
                }
                let _ = stdout.flush();
            } else if let Some(rest) = line.strip_prefix("SCROLL ") {
                match rest.trim().parse::<f64>() {
                    Ok(dy) => {
                        let max_scroll = (page.content_height(width) - height as f64).max(0.0);
                        scroll_top = (scroll_top + dy).clamp(0.0, max_scroll);
                        writer.publish(&page.render(&renderer, width, height, scroll_top));
                        let _ = writeln!(stdout, "SCROLLED");
                    }
                    Err(_) => {
                        let _ = writeln!(stdout, "ERROR SCROLL requires a numeric delta");
                    }
                }
                let _ = stdout.flush();
            } else if let Some(rest) = line.strip_prefix("SET_FPS_CAP ") {
                match rest.trim().parse::<u32>() {
                    Ok(fps) if fps >= 1 => {
                        frame_interval = Duration::from_nanos(1_000_000_000 / fps as u64);
                        let _ = writeln!(stdout, "FPS_CAP_SET {fps}");
                    }
                    _ => {
                        let _ = writeln!(stdout, "ERROR fps cap must be a positive integer");
                    }
                }
                let _ = stdout.flush();
            } else if line == "QUIT" {
                break 'render_loop;
            }
        }

        if paused {
            // No timer pumping, no render, no publish - genuinely idle,
            // not "render the same frame every tick". A short fixed sleep
            // (not `frame_interval`, which could be very large under a low
            // fps cap) keeps `PING`/`RESUME`/`QUIT` responsive without
            // busy-spinning the command-drain loop above.
            thread::sleep(Duration::from_millis(50));
            continue 'render_loop;
        }

        page.ctx.run_pending_timers();
        writer.publish(&page.render(&renderer, width, height, scroll_top));

        // Fixed-cadence scheduling, not `sleep(frame_interval)` in a loop -
        // that drifts by however long each tick's own work took. If a tick
        // ran long enough to miss its slot entirely, resync to now rather
        // than trying to render the missed frames back-to-back (a real
        // vsync loop drops frames under load; it doesn't queue them up).
        // `frame_interval` is read fresh each tick, not captured once, so a
        // `SET_FPS_CAP` takes effect on the very next tick.
        let now = Instant::now();
        if next_tick > now {
            thread::sleep(next_tick - now);
            next_tick += frame_interval;
        } else {
            next_tick = now + frame_interval;
        }
    }
}
