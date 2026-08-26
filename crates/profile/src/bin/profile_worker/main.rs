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
//! Real `Content-Security-Policy` delivery too (`resolve_document`/
//! `collect_meta_csp_policies`, both consumed by `Page::load` before any
//! page script runs): the document response's own CSP headers and every
//! `<meta http-equiv="Content-Security-Policy">` tag in the parsed HTML
//! are handed to `js_runtime::Context::add_csp_policy` as separate
//! policies, so a fetched page's own `fetchSync`/`fetch()`/XHR targets
//! are gated by its server's policy — see `js-runtime`'s `csp` module for
//! what a policy actually restricts in this engine.
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
//! - `EVAL <script>` -> evaluates arbitrary JS in the current page's own
//!   context (the automation/devtools-console entry point this protocol
//!   previously had no equivalent of - every other command was a fixed,
//!   hard-coded mutation, so a test couldn't trigger e.g. a stylesheet
//!   mutation and watch it repaint; see `profile::Profile::evaluate`).
//!   The current frame is re-rendered and republished after evaluation in
//!   either outcome, so whatever the script mutated is visible in pixels
//!   by the time the reply arrives. Replies `EVALUATED <result>` with the
//!   script's stringified completion value (newlines flattened, same as
//!   error messages), or `ERROR <stringified exception>` if evaluation
//!   threw - what a devtools console shows for an uncaught error, not just
//!   that something did (an Error's own message, a thrown string verbatim).
//!   Not gated by CSP: this is host-driven evaluation (a devtools
//!   console), not the page loading its own script - a real browser's
//!   devtools likewise bypasses the page's policy. Scripts can't contain
//!   newlines (newline-delimited protocol); `profile::Profile::evaluate`
//!   rejects those before they'd corrupt the stream.
//! - `CONSOLE` -> drains every `console.*` message the page has produced
//!   so far (`console.log/info/warn/error/debug`, plus uncaught script
//!   errors reported at error level by `Context::eval` itself) and replies
//!   `MESSAGES <entries>` with each rendered as `<level>:<text>`,
//!   `|`-separated (message text has newlines and `|` flattened, since the
//!   reply must be one protocol line), or bare `MESSAGES` when nothing was
//!   logged. Draining: a second `CONSOLE` only returns messages logged
//!   after the first - devtools-console semantics, and part of what keeps
//!   the buffer bounded. See `profile::Profile::console`.
//! - `QUIT` -> exits cleanly
//! - anything else -> ignored
//!
//! Split into one file per cohesive responsibility group (SRP), same
//! approach as `js-runtime`'s `dom_bindings` directory module:
//! - [`network`]: cookie-aware fetch plus this process's own optional
//!   proxy/DNS CLI-argument parsing.
//! - [`page_source`]: pulling CSS/script/image sources out of a parsed DOM
//!   and merging stylesheets (including `@import`).
//! - [`document_load`]: resolving a page's source into a real
//!   [`document_load::LoadedDocument`] and then a full [`page::Page`].
//! - [`input_commands`]: simulating user input (click/focus/blur/fill/
//!   type/tab) against a live page.
//! - [`layout_snapshot`]: flattening a computed `LayoutBox` tree into the
//!   wire-shaped data `getBoundingClientRect`/`getComputedStyle` need.
//! - [`page`]: the `Page` struct itself — DOM + stylesheet + JS context +
//!   cached layout, plus `render`/`hit_test_at`.
//!
//! `main()` stays here as the thin entry point: CLI-argument parsing, the
//! stdin command-protocol loop, and the fixed-cadence render loop
//! dispatching into the modules above.
mod document_load;
mod input_commands;
mod layout_snapshot;
mod network;
mod page;
mod page_source;

use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use js_runtime::Runtime;
use render::GpuRenderer;

use document_load::{load_source, PageSource};
use input_commands::{dispatch_click, dispatch_click_at, fill_element, tab_focus, type_key};
use network::{parse_dns_arg, parse_proxy_arg};

const TARGET_FPS: u32 = 60;
const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / TARGET_FPS as u64);

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
    let gpu_adapter: Option<usize> = args
        .get(6)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse().ok());

    // One storage root per worker process, keyed by shmem name (already
    // unique per spawned profile) so two profiles never share cookies/
    // localStorage, then further split by host under `Page::load` -
    // real per-origin partitioning. Doesn't persist across the worker
    // process's own lifetime yet - see `Page::load`'s doc.
    let storage_root = std::env::temp_dir()
        .join("nimble-profile-storage")
        .join(shmem_name);

    let runtime = Runtime::new();
    let mut current_source = PageSource::Demo;
    let (mut page, _) = load_source(
        &runtime,
        &current_source,
        width as f64,
        &storage_root,
        proxy.as_ref(),
        dns_server,
    );
    let renderer = match gpu_adapter {
        Some(index) => GpuRenderer::new_with_adapter(index),
        None => GpuRenderer::new(),
    };

    let mut writer = ipc::FrameWriter::new(shmem_name, width, height)
        .expect("failed to create/open shared memory");
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
                let (loaded, error) = load_source(
                    &runtime,
                    &current_source,
                    width as f64,
                    &storage_root,
                    proxy.as_ref(),
                    dns_server,
                );
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
                let (loaded, error) = load_source(
                    &runtime,
                    &current_source,
                    width as f64,
                    &storage_root,
                    proxy.as_ref(),
                    dns_server,
                );
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
                let coords = parts
                    .next()
                    .zip(parts.next())
                    .and_then(|(x, y)| Some((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?)));
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
                        let _ = writeln!(
                            stdout,
                            "ERROR no element focused - CLICK_AT an <input>/<textarea> first"
                        );
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
                        let _ =
                            writeln!(stdout, "ERROR no focusable element with an id on this page");
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
            } else if let Some(script) = line.strip_prefix("EVAL ") {
                let script = script.trim();
                // Rendered in both outcomes - a script that mutated the DOM
                // (or an adopted stylesheet) before throwing still changed
                // real state worth painting, same convention CLICK's handler
                // follows when the dispatch itself fails. A NUL byte is
                // rejected up front rather than passed to `Context::eval`,
                // whose `CString::new` conversion panics on one - a hostile/
                // buggy stdin writer must not be able to crash this process.
                if script.contains('\0') {
                    let _ = writeln!(stdout, "ERROR script must not contain NUL bytes");
                    let _ = stdout.flush();
                } else {
                    let result = page.ctx.eval(script, "<pane eval>");
                    writer.publish(&page.render(&renderer, width, height, scroll_top));
                    match result {
                        Ok(value) => {
                            let _ = writeln!(stdout, "EVALUATED {}", value.replace('\n', " "));
                        }
                        Err(e) => {
                            // The real stringified exception (an Error's own
                            // message, a thrown string verbatim) - a devtools
                            // console shows what threw, not just that
                            // something did.
                            let _ = writeln!(stdout, "ERROR {}", e.0.replace('\n', " "));
                        }
                    }
                    let _ = stdout.flush();
                }
            } else if line == "CONSOLE" {
                // Drains every `console.*` message the page has produced so
                // far - load-time scripts' output AND anything EVAL'd since
                // (uncaught script errors land in the same buffer, reported
                // by `Context::eval` itself). Read-only as far as rendering
                // goes: no re-render, no frame publish. Draining means a
                // second CONSOLE only returns messages logged after the
                // first - devtools-console semantics, and what bounds this
                // from growing forever across a long session (the buffer is
                // additionally capped inside js-runtime). Message text is
                // flattened (newlines -> spaces, our separator -> slashes)
                // because the reply must be one protocol line.
                let joined = page
                    .ctx
                    .take_console_messages()
                    .iter()
                    .map(|m| {
                        format!(
                            "{}:{}",
                            m.level.as_str(),
                            m.text.replace('\n', " ").replace('|', "/")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                if joined.is_empty() {
                    let _ = writeln!(stdout, "MESSAGES");
                } else {
                    let _ = writeln!(stdout, "MESSAGES {joined}");
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
