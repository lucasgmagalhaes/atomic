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
//! `url` crate (`resolve_stylesheet_url` — relative, root-relative, and
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
//! Command protocol over stdin (newline-delimited, one command per line),
//! read on a dedicated thread and drained non-blockingly by the render
//! loop each tick (so a slow/absent command stream never stalls
//! rendering, and a burst of frames never stalls a command response by
//! more than one tick, ~16ms):
//! - `PING` -> replies `PONG` on stdout (liveness check)
//! - `RELOAD` -> re-fetches/re-renders whatever page is currently loaded
//!   (the built-in demo page, or the last `NAVIGATE`d URL — including
//!   retrying one that previously failed), creates a fresh JS context
//!   (resetting any JS state - matches a real navigation); replies
//!   `RELOADED` once that's done, or `ERROR <message>` if the (re-)fetch
//!   failed, same as `NAVIGATE`
//! - `NAVIGATE <url>` -> fetches `url` and renders it as the new page;
//!   replies `NAVIGATED` on success or `ERROR <message>` on failure (bad
//!   URL, network error, ...) — either way an in-page error message is
//!   rendered too, not just reported over the protocol, so the frame
//!   itself never silently goes stale
//! - `CLICK <#id>` -> dispatches a real `"click"` event (via the existing
//!   `Node.prototype.dispatchEvent` JS binding) at the element with that
//!   id; replies `CLICKED` or `ERROR <message>` (no such id, or only
//!   `#id` selectors are supported at all — this engine has no general
//!   CSS selector query beyond `dom::Dom::find_by_id`)
//! - `FILL <#id> <value>` -> sets that element's `textContent` to `value`
//!   and replies `FILLED`/`ERROR <message>` the same way. A real
//!   `<input>`'s `value` is a distinct property from its children/text -
//!   this engine models neither `HTMLInputElement` nor a `value` property
//!   at all yet, so `textContent` is the closest existing primitive
//!   (`js-runtime`'s only settable DOM string), not a faithful `.value`
//!   assignment. `value` may not contain a newline (this protocol is
//!   newline-delimited) - `profile::Profile::fill` rejects that before it
//!   would corrupt the stream.
//! - `QUIT` -> exits cleanly
//! - anything else -> ignored
use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use css::{parse_stylesheet, Stylesheet};
use dom::{Dom, NodeData, NodeId};
use js_runtime::{Context, Runtime};
use layout_engine::{build_box_tree_with_viewport, layout_block};
use render::{build_display_list, build_glyph_list, composite_glyphs, GpuRenderer};

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
    if let NodeData::Element { tag, attributes } = &n.data {
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
/// stance elsewhere (see `resolve_stylesheet_url`).
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
fn resolve_stylesheet_url(base_url: Option<&str>, href: &str) -> Option<String> {
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
/// `base_url` via [`resolve_stylesheet_url`] — a `<link>` that doesn't
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
        if let Some(resolved) = resolve_stylesheet_url(base_url, &import.url) {
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
            CssSource::Link(href) => resolve_stylesheet_url(base_url, &href)
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

/// Sets the `#id` element's `textContent` to `value` via the existing
/// `Node.prototype.textContent` setter — see this file's own doc comment
/// on the `FILL` command for why that's a deviation from a real
/// `HTMLInputElement.value` assignment, not an equivalent of one.
fn fill_element(ctx: &Context, selector: &str, value: &str) -> Result<(), String> {
    let id = require_id_selector(selector)?;
    let script = format!(
        "(function(){{ var el = document.getElementById({id}); if (el === null) throw {missing}; el.textContent = {value}; }})();",
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

        let ctx = match storage_host {
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
        if run_demo_script {
            let _ = ctx.eval(DEMO_SCRIPT, "<profile-worker demo>");
        }
        Page { ctx, html_el, sheet }
    }

    /// Re-layouts and re-rasterizes from the DOM's *current* state (which
    /// may have just been mutated by a JS timer callback) against an
    /// already-initialized `renderer` — creating a `GpuRenderer` opens a
    /// real GPU device/adapter, expensive enough that doing it every tick
    /// would itself become the vsync loop's bottleneck instead of the
    /// fixed frame interval.
    fn render(&self, renderer: &GpuRenderer, width: u32, height: u32) -> Vec<u8> {
        let dom = self.ctx.dom().expect("Page always constructs its Context via with_dom");
        let mut tree =
            build_box_tree_with_viewport(dom, self.html_el, &self.sheet, width as f64).expect("parsed HTML always produces a box");
        layout_block(&mut tree, width as f64, 0.0, 0.0);

        let rects = build_display_list(&tree);
        let glyphs = build_glyph_list(&tree);

        let mut pixels = renderer.render_to_rgba(&rects, width, height, [0.08, 0.09, 0.13, 1.0]);
        composite_glyphs(&mut pixels, width, height, &glyphs);
        pixels
    }
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
    if args.len() < 4 || args.len() > 6 {
        eprintln!("usage: profile-worker <shmem-name> <width> <height> [proxy: host:port or user:pass@host:port] [dns-server: host:port]");
        std::process::exit(2);
    }
    let shmem_name = &args[1];
    let width: u32 = args[2].parse().expect("width must be a positive integer");
    let height: u32 = args[3].parse().expect("height must be a positive integer");
    let proxy = parse_proxy_arg(args.get(4).map(String::as_str));
    let dns_server = parse_dns_arg(args.get(5).map(String::as_str));

    // One storage root per worker process, keyed by shmem name (already
    // unique per spawned profile) so two profiles never share cookies/
    // localStorage, then further split by host under `Page::load` -
    // real per-origin partitioning. Doesn't persist across the worker
    // process's own lifetime yet - see `Page::load`'s doc.
    let storage_root = std::env::temp_dir().join("nimble-profile-storage").join(shmem_name);

    let runtime = Runtime::new();
    let mut current_source = PageSource::Demo;
    let (mut page, _) = load_source(&runtime, &current_source, width as f64, &storage_root, proxy.as_ref(), dns_server);
    let renderer = GpuRenderer::new();

    let mut writer = ipc::FrameWriter::new(shmem_name, width, height).expect("failed to create/open shared memory");
    writer.publish(&page.render(&renderer, width, height));

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
    let mut next_tick = Instant::now() + FRAME_INTERVAL;

    'render_loop: loop {
        while let Ok(line) = cmd_rx.try_recv() {
            let line = line.trim();
            if line == "PING" {
                let _ = writeln!(stdout, "PONG");
                let _ = stdout.flush();
            } else if line == "RELOAD" {
                let (loaded, error) = load_source(&runtime, &current_source, width as f64, &storage_root, proxy.as_ref(), dns_server);
                page = loaded;
                writer.publish(&page.render(&renderer, width, height));
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
                current_source = PageSource::Url(url.trim().to_string());
                let (loaded, error) = load_source(&runtime, &current_source, width as f64, &storage_root, proxy.as_ref(), dns_server);
                page = loaded;
                writer.publish(&page.render(&renderer, width, height));
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
                writer.publish(&page.render(&renderer, width, height));
                match result {
                    Ok(()) => {
                        let _ = writeln!(stdout, "CLICKED");
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
                writer.publish(&page.render(&renderer, width, height));
                match result {
                    Ok(()) => {
                        let _ = writeln!(stdout, "FILLED");
                    }
                    Err(message) => {
                        let _ = writeln!(stdout, "ERROR {}", message.replace('\n', " "));
                    }
                }
                let _ = stdout.flush();
            } else if line == "QUIT" {
                break 'render_loop;
            }
        }

        page.ctx.run_pending_timers();
        writer.publish(&page.render(&renderer, width, height));

        // Fixed-cadence scheduling, not `sleep(FRAME_INTERVAL)` in a loop -
        // that drifts by however long each tick's own work took. If a tick
        // ran long enough to miss its slot entirely, resync to now rather
        // than trying to render the missed frames back-to-back (a real
        // vsync loop drops frames under load; it doesn't queue them up).
        let now = Instant::now();
        if next_tick > now {
            thread::sleep(next_tick - now);
            next_tick += FRAME_INTERVAL;
        } else {
            next_tick = now + FRAME_INTERVAL;
        }
    }
}
