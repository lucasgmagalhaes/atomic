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
//! Scope cuts that remain: only absolute `http(s)://` `<link>` hrefs are
//! fetched (no base-URL-relative resolution — this workspace has no URL
//! join/parse library yet), no `@import`, no media queries, and every
//! stylesheet fetch is sequential and blocking (same "the fetch blocks
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
//! - `QUIT` -> exits cleanly
//! - anything else -> ignored (unrecognized commands are not an error;
//!   real input commands beyond navigation aren't implemented yet)
use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use css::{parse_stylesheet, Stylesheet};
use dom::{Dom, NodeData, NodeId};
use js_runtime::{Context, Runtime};
use layout_engine::{build_box_tree, layout_block};
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
const DEMO_SCRIPT: &str = r#"
var tickCount = 0;
var rafCount = 0;
function onTick() {
    tickCount++;
    document.getElementById('counter').textContent = 'tick ' + tickCount + ' raf ' + rafCount;
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

/// Resolves every CSS source in `dom` (rooted at `root`) into one cascaded
/// [`Stylesheet`]: [`BASE_STYLESHEET_SRC`] first, then each `<style>`/
/// `<link>` source in document order — a `<link>` whose href isn't an
/// absolute `http(s)://` URL is silently skipped (see the module doc's
/// scope cut), matching `resolve_html`'s own "best-effort, never panics
/// on a real page" stance rather than failing the whole page load over
/// one bad stylesheet reference.
fn build_stylesheet(dom: &Dom, root: NodeId) -> Stylesheet {
    let mut sources = Vec::new();
    collect_css_sources(dom, root, &mut sources);

    let mut sheet = parse_stylesheet(BASE_STYLESHEET_SRC);
    for source in sources {
        let css_text = match source {
            CssSource::Inline(text) => Some(text),
            CssSource::Link(href) if href.starts_with("http://") || href.starts_with("https://") => {
                net::get(&href).ok().map(|response| String::from_utf8_lossy(&response.body).into_owned())
            }
            CssSource::Link(_) => None,
        };
        if let Some(css_text) = css_text {
            sheet.rules.extend(parse_stylesheet(&css_text).rules);
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
fn resolve_html(source: &PageSource) -> Result<String, String> {
    match source {
        PageSource::Demo => Ok(DEMO_HTML.to_string()),
        PageSource::Url(url) => net::get(url)
            .map(|response| String::from_utf8_lossy(&response.body).into_owned())
            .map_err(|e| e.to_string()),
    }
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
    /// `#counter` element for `DEMO_SCRIPT` to find.
    fn load(runtime: &'rt Runtime, html: &str, run_demo_script: bool) -> Self {
        let (dom, html_el) = html::parse_to_html_element(html);
        let sheet = build_stylesheet(&dom, html_el);
        let ctx = Context::with_dom(runtime, dom);
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
        let mut tree = build_box_tree(dom, self.html_el, &self.sheet).expect("parsed HTML always produces a box");
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
fn load_source<'rt>(runtime: &'rt Runtime, source: &PageSource) -> (Page<'rt>, Option<String>) {
    match resolve_html(source) {
        Ok(html) => {
            let run_demo_script = matches!(source, PageSource::Demo);
            (Page::load(runtime, &html, run_demo_script), None)
        }
        Err(message) => {
            let url = match source {
                PageSource::Demo => "the demo page",
                PageSource::Url(url) => url,
            };
            (Page::load(runtime, &error_page_html(url, &message), false), Some(message))
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: profile-worker <shmem-name> <width> <height>");
        std::process::exit(2);
    }
    let shmem_name = &args[1];
    let width: u32 = args[2].parse().expect("width must be a positive integer");
    let height: u32 = args[3].parse().expect("height must be a positive integer");

    let runtime = Runtime::new();
    let mut current_source = PageSource::Demo;
    let (mut page, _) = load_source(&runtime, &current_source);
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
                let (loaded, error) = load_source(&runtime, &current_source);
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
                let (loaded, error) = load_source(&runtime, &current_source);
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
