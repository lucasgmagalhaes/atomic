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
//! published frame. Still not a working browser tab: the HTML source is a
//! fixed demo string (not fetched from a URL — `net` isn't wired in here),
//! there's no `<style>`/`<link>` extraction (the stylesheet is applied
//! separately), and the demo script that drives the counter is a hardcoded
//! Rust string `eval`'d after parsing, not read out of a `<script>` tag
//! (html5ever parses `<script>` content as plain DOM text — nothing
//! executes it).
//!
//! Command protocol over stdin (newline-delimited, one command per line),
//! now read on a dedicated thread and drained non-blockingly by the render
//! loop each tick (so a slow/absent command stream never stalls
//! rendering, and a burst of frames never stalls a command response by
//! more than one tick, ~16ms):
//! - `PING` -> replies `PONG` on stdout (liveness check)
//! - `RELOAD` -> re-parses the page, creates a fresh JS context (resetting
//!   any JS state - matches a real navigation), re-evaluates the demo
//!   script, and lets the loop's next tick publish the new frame; replies
//!   `RELOADED` once that's done
//! - `QUIT` -> exits cleanly
//! - anything else -> ignored (unrecognized commands are not an error;
//!   real input/navigation commands aren't implemented yet)
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

/// Runs once per (re)load: increments a counter and writes it into
/// `#counter`'s text every 50ms via `setInterval`, and separately bumps a
/// `requestAnimationFrame`-driven counter every tick to prove both timer
/// kinds are actually being pumped by the host loop, not just accepted
/// and ignored.
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

/// One (re)loadable "page": the parsed DOM's root `<html>` element plus
/// the JS context mutating it. Rebuilt from scratch on every `RELOAD`,
/// same as a real navigation resetting a page's JS state.
struct Page<'rt> {
    ctx: Context<'rt>,
    html_el: NodeId,
}

impl<'rt> Page<'rt> {
    fn load(runtime: &'rt Runtime) -> Self {
        let (dom, html_el) = html::parse_to_html_element(DEMO_HTML);
        let ctx = Context::with_dom(runtime, dom);
        let _ = ctx.eval(DEMO_SCRIPT, "<profile-worker demo>");
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
    let mut page = Page::load(&runtime);
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
            match line.trim() {
                "PING" => {
                    let _ = writeln!(stdout, "PONG");
                    let _ = stdout.flush();
                }
                "RELOAD" => {
                    page = Page::load(&runtime);
                    writer.publish(&page.render(&sheet, &renderer, width, height));
                    let _ = writeln!(stdout, "RELOADED");
                    let _ = stdout.flush();
                }
                "QUIT" => break 'render_loop,
                _ => {}
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
