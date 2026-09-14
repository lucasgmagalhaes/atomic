//! Bundles the render loop's mutable state (page, viewport, scroll,
//! pending navigation, timing) into [`WorkerState`] so the giant stdin
//! command dispatch in `main.rs` could be split across files by concern.
//! Split into `lifecycle.rs` (`PING`/`PAUSE`/`RESUME`/`SET_FPS_CAP`/
//! `RESIZE`/`QUIT`), `page_nav.rs` (`RELOAD`/`NAVIGATE`/`EVAL`/`CONSOLE`),
//! and `input.rs` (`CLICK`/`CLICK_AT`/`KEY`/`TAB`/`TAB_REVERSE`/`FILL`/
//! `SCROLL`) — this file keeps the struct itself, `sync_scroll`,
//! `navigate_if_requested`, and the top-level dispatcher,
//! `handle_command`.

use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use js_runtime::Runtime;
use render::GpuRenderer;

use super::document_load::PageSource;
use super::page;

mod input;
mod lifecycle;
mod page_nav;

/// Result of trying one command handler against a line: `No` if the line
/// didn't match anything this handler understands (the dispatcher tries
/// the next handler), `Handled` if it did (whatever reply/render already
/// happened), or `Quit` for `QUIT` specifically (unwinds the render loop).
pub(super) enum Handled {
    No,
    Handled,
    Quit,
}

pub(super) struct WorkerState<'rt> {
    pub(super) shmem_name: String,
    pub(super) runtime: &'rt Runtime,
    pub(super) page: page::Page<'rt>,
    pub(super) current_source: PageSource,
    pub(super) storage_root: PathBuf,
    pub(super) proxy: Option<net::ProxyConfig>,
    pub(super) dns_server: Option<SocketAddr>,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) renderer: GpuRenderer,
    pub(super) writer: ipc::FrameWriter,
    pub(super) focused_id: Option<String>,
    /// Last `CLICK_AT` target id + timestamp, for real `dblclick`
    /// detection - see `input_commands::click::dispatch_click_at`'s own
    /// doc.
    pub(super) last_click: Option<(String, Instant)>,
    /// Real in-progress drag: `(source_id, carried "text/plain" data)`,
    /// set by a real, uncanceled `DRAG_START` and consumed by the next
    /// `DROP_AT` (`input_commands::drag`'s own doc has the full real
    /// `dragstart`/`dragover`/`drop`/`dragend` sequence). `None` when no
    /// drag is in progress.
    pub(super) drag_source: Option<(String, String)>,
    pub(super) scroll_top: f64,
    pub(super) resize_count: u32,
    pub(super) frame_interval: Duration,
    pub(super) paused: bool,
    pub(super) next_tick: Instant,
}

impl<'rt> WorkerState<'rt> {
    /// Reconciles `scroll_top` (the loop's own render/hit-test offset) with
    /// whatever `page.ctx`'s real `window.scrollY` currently holds — a
    /// page's own script (a `scrollTo` call, or a `"load"`/timer/click-
    /// handler that runs one) can change that independently of any
    /// `SCROLL` command, so this must run before every paint, not just
    /// after `SCROLL` itself. Clamps against real content height (same
    /// `[0, content_height - viewport_height]` range `SCROLL`'s own
    /// handling already enforced) and writes the clamped result back into
    /// `page.ctx` too, so a page that requested an out-of-range offset
    /// observes the corrected value on its very next read — the host is
    /// authoritative, script only requests, same split a real
    /// compositor-driven scroll has.
    pub(super) fn sync_scroll(&mut self) {
        let requested = self.page.ctx.scroll_y();
        let max_scroll =
            (self.page.content_height(self.width, self.height) - self.height as f64).max(0.0);
        let clamped = requested.clamp(0.0, max_scroll);
        self.scroll_top = clamped;
        self.page.ctx.set_scroll_y(clamped);
    }

    /// Checks for and performs a pending `<a href>` click-navigation
    /// request (see `js_runtime::Context::take_pending_navigation`'s own
    /// doc) — called after any command that could dispatch a real click
    /// (`CLICK`/`CLICK_AT`/`EVAL`). This *is* a real navigation, same
    /// `fire_before_unload` gate, `load_source`, and focus/scroll reset
    /// `NAVIGATE`'s own handler already uses — just host-invisible in the
    /// sense that no explicit `NAVIGATE` command triggered it, only an
    /// in-page click. A silent no-op if nothing was requested, the href
    /// doesn't resolve to a real `http(s)` URL (see
    /// `page_source::resolve_url`), or `beforeunload` cancels it — the
    /// click itself already succeeded/replied by the time this runs; it
    /// only affects whether a *subsequent* published frame shows a new
    /// page.
    pub(super) fn navigate_if_requested(&mut self) {
        let Some(href) = self.page.ctx.take_pending_navigation() else {
            return;
        };
        let base_url = match &self.current_source {
            PageSource::Demo => None,
            PageSource::Url(url) => Some(url.as_str()),
        };
        let Some(resolved) = super::page_source::resolve_url(base_url, &href) else {
            return;
        };
        if !self.page.ctx.fire_before_unload() {
            return;
        }
        self.current_source = PageSource::Url(resolved);
        let (loaded, _error) = super::document_load::load_source(
            self.runtime,
            &self.current_source,
            self.width as f64,
            &self.storage_root,
            self.proxy.as_ref(),
            self.dns_server,
        );
        self.page = loaded;
        self.focused_id = None;
        self.scroll_top = 0.0;
        self.sync_scroll();
        self.writer.publish(&self.page.render(
            &self.renderer,
            self.width,
            self.height,
            self.scroll_top,
        ));
    }

    /// Tries each command group's handler in turn (lifecycle, then page/
    /// navigation, then input) against `line`, stopping at the first that
    /// recognizes it. Returns `true` if the render loop should exit
    /// (`QUIT`), `false` otherwise (including "line matched nothing" —
    /// same "anything else -> ignored" behavior the original single
    /// if/else-if chain had).
    pub(super) fn handle_command(&mut self, stdout: &mut impl Write, line: &str) -> bool {
        match self.handle_lifecycle(stdout, line) {
            Handled::Quit => return true,
            Handled::Handled => return false,
            Handled::No => {}
        }
        if let Handled::Handled = self.handle_page_nav(stdout, line) {
            return false;
        }
        if let Handled::Handled = self.handle_input(stdout, line) {
            return false;
        }
        false
    }
}
