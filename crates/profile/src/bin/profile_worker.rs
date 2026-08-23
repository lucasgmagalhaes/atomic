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
//! actual response — this is no longer just a fixed demo string. Still
//! cut down from a working browser tab: no `<style>`/`<link>` extraction
//! (a real fetched page's own stylesheets are never applied — only the
//! one hardcoded demo stylesheet exists, which won't match a real page's
//! markup at all, so navigated pages render unstyled: real layout against
//! a real parsed DOM, just with every element at its CSS initial values),
//! no redirect following (`net::get`'s own scope), and the fetch blocks
//! this process's render loop for its duration (a real browser fetches
//! off the render thread and shows a loading state in the meantime; this
//! one just misses a few frame ticks - acceptable at this scope since
//! there's nothing animating during a fetch anyway, but a real regression
//! if that ever changes).
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
use dom::NodeId;
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

/// One (re)loadable "page": the parsed DOM's root `<html>` element plus
/// the JS context mutating it. Rebuilt from scratch on every navigation/
/// reload, same as a real navigation resetting a page's JS state.
struct Page<'rt> {
    ctx: Context<'rt>,
    html_el: NodeId,
}

impl<'rt> Page<'rt> {
    /// Builds a page from `html`. `run_demo_script` should only be `true`
    /// for the built-in demo page - a real fetched page has no
    /// `#counter` element for `DEMO_SCRIPT` to find.
    fn load(runtime: &'rt Runtime, html: &str, run_demo_script: bool) -> Self {
        let (dom, html_el) = html::parse_to_html_element(html);
        let ctx = Context::with_dom(runtime, dom);
        if run_demo_script {
            let _ = ctx.eval(DEMO_SCRIPT, "<profile-worker demo>");
        }
        Page { ctx, html_el }
    }

    /// Re-layouts and re-rasterizes from the DOM's *current* state (which
    /// may have just been mutated by a JS timer callback) against an
    /// already-initialized `renderer` — creating a `GpuRenderer` opens a
    /// real GPU device/adapter, expensive enough that doing it every tick
    /// would itself become the vsync loop's bottleneck instead of the
    /// fixed frame interval.
    fn render(&self, sheet: &Stylesheet, renderer: &GpuRenderer, width: u32, height: u32) -> Vec<u8> {
        let dom = self.ctx.dom().expect("Page always constructs its Context via with_dom");
        let mut tree = build_box_tree(dom, self.html_el, sheet).expect("parsed HTML always produces a box");
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

    let sheet = parse_stylesheet(
        "#container { background-color: #1a1c2b; padding: 20px; } \
         p { color: #ffffff; font-size: 24px; }",
    );

    let runtime = Runtime::new();
    let mut current_source = PageSource::Demo;
    let (mut page, _) = load_source(&runtime, &current_source);
    let renderer = GpuRenderer::new();

    let mut writer = ipc::FrameWriter::new(shmem_name, width, height).expect("failed to create/open shared memory");
    writer.publish(&page.render(&sheet, &renderer, width, height));

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
                writer.publish(&page.render(&sheet, &renderer, width, height));
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
                writer.publish(&page.render(&sheet, &renderer, width, height));
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
        writer.publish(&page.render(&sheet, &renderer, width, height));

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
