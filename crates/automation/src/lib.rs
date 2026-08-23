//! User automation scripts: the `pane`/`every`/`on`/cron surface the mockup
//! shows (login automático, claim idle, watchdog reconexão) that no crate
//! owns yet — see `CLAUDE.md`'s spec-gap table, line 255/260. Scripts are
//! plain JS run in their own `js_runtime::Context` (real quickjs, same
//! engine every page uses), not a new language — the mockup's own sample
//! scripts are already JS (`pane.goto`, `pane.fill`, `pane.click`,
//! `every()`, `on()`).
//!
//! Scope of this pass: wiring, not full coverage. `pane.goto` is real (maps
//! to `profile::Profile::navigate`). `pane.fill`/`pane.click` are not —
//! `profile-worker`'s stdin protocol only understands
//! `PING`/`RELOAD`/`NAVIGATE`/`QUIT` (see `profile::Profile`'s doc comment
//! and the spec's own gap on line 252, input sync between panes), so they
//! throw a JS exception naming the missing protocol command instead of
//! silently no-opping. `every()` is a plain alias for the already-real
//! `setInterval` (js-runtime's `timers` module) rather than a new
//! primitive. `on`/`emit` and `cron` are real, host-driven event/schedule
//! primitives — see `events` and `cron` modules.
mod cron;
mod events;
mod pane;

pub use cron::CronError;

use js_runtime::{Context, EvalError, Runtime};
use std::collections::HashMap;

/// One running automation script: its own JS context, the set of panes
/// (named, *borrowed* `profile::Profile` handles — see below) it's allowed
/// to control, and the cron/event state `tick()` drives.
pub struct AutomationEngine<'rt, 'p> {
    ctx: Context<'rt>,
    // Boxed so the heap address stays valid for the raw pointer stashed via
    // `JS_SetContextOpaque` even though `AutomationEngine` itself may move
    // (e.g. returned out of `new`) — same convention as js-runtime's own
    // `Context::_host_state`.
    //
    // Borrowed (`&'p mut Profile`), not owned: a real host (`apps/shell`)
    // already owns each running profile for rendering/IPC — a `Profile`'s
    // stdin/stdout pipe has exactly one writer, so it can't also be handed
    // to this engine by value without taking it away from whatever's
    // displaying it. Lending a `&mut` for the duration of a script run (or
    // a `tick()`) lets the same profile serve both.
    panes: Box<HashMap<String, &'p mut profile::Profile>>,
}

// A plain alias, not a wrapper — so `every` keeps `setInterval`'s real
// argument order, `(callback, delayMs)`. The mockup's own sample scripts
// never show `every()` called with both arguments together, so this is a
// real design call: pick JS's existing convention rather than inventing a
// `(delayMs, callback)` order that would silently misfire (a number where
// `JS_Call` expects a function throws, not a clear error) if a script ever
// assumed the other one.
const PRELUDE: &str = r#"globalThis.every = setInterval;"#;

impl<'rt, 'p> AutomationEngine<'rt, 'p> {
    /// `panes` are the profiles this script is allowed to name via
    /// `pane("name")` — deliberately explicit rather than "every profile in
    /// the workspace": a script's blast radius should be whatever the host
    /// (e.g. the Settings > Automation "script sandbox" scope the spec
    /// flags as a gap on line 260) grants it, not everything running. A
    /// host with a `workspace::WorkspaceManager` (`apps/shell`) would build
    /// this from the active workspace's profile ids, keeping only the ones
    /// it actually has a live `Profile` for.
    pub fn new(runtime: &'rt Runtime, panes: HashMap<String, &'p mut profile::Profile>) -> Self {
        let ctx = Context::new(runtime);
        let mut panes = Box::new(panes);
        unsafe {
            pane::register(ctx.as_raw(), panes.as_mut() as *mut HashMap<String, &mut profile::Profile>);
            events::register(ctx.as_raw());
            cron::register(ctx.as_raw());
        }
        let engine = AutomationEngine { ctx, panes };
        engine
            .ctx
            .eval(PRELUDE, "automation-prelude.js")
            .expect("prelude is static and must not fail to eval");
        engine
    }

    /// Runs a user script's top-level code once (registers its
    /// `every`/`on`/cron callbacks; doesn't fire any of them yet).
    pub fn run(&self, source: &str, filename: &str) -> Result<String, EvalError> {
        self.ctx.eval(source, filename)
    }

    /// One cooperative tick: pumps due timers/`requestAnimationFrame`
    /// (`every` included, since it's just `setInterval`) and due cron
    /// triggers. Same "no real event loop yet, the host must call this"
    /// deviation `js_runtime::Context::run_pending_timers` already
    /// documents — a real per-schedule engine belongs with `profile`/`ipc`'s
    /// eventual per-tab event loop, not invented ahead of it here.
    pub fn tick(&self) {
        self.ctx.run_pending_timers();
        unsafe { cron::pump(self.ctx.as_raw()) };
    }

    /// Fires every `on(name, ...)` listener registered for `name` — the
    /// host's hook for events a script can't observe on its own (e.g. a
    /// watchdog noticing a pane went unresponsive). Not a DOM event: this
    /// context has no `dom::Dom` (see `Context::new` vs `Context::with_dom`
    /// in `js-runtime`), so there's no `dispatchEvent` overlap to conflict
    /// with.
    pub fn emit(&self, name: &str) {
        unsafe { events::emit(self.ctx.as_raw(), name) };
    }

    pub fn pane_names(&self) -> impl Iterator<Item = &str> {
        self.panes.keys().map(String::as_str)
    }
}

impl Drop for AutomationEngine<'_, '_> {
    fn drop(&mut self) {
        // Must run before `self.ctx` itself drops (which calls
        // `JS_FreeContext`) — same ordering requirement `timers::cleanup`/
        // `fetch_async::cleanup` already document in js-runtime.
        unsafe {
            events::cleanup(self.ctx.as_raw());
            cron::cleanup(self.ctx.as_raw());
        }
    }
}
