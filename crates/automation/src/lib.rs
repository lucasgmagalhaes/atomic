//! User automation scripts: the `pane`/`every`/`on`/cron surface the mockup
//! shows (login automático, claim idle, watchdog reconexão) that no crate
//! owns yet — see `CLAUDE.md`'s spec-gap table, line 255/260. Scripts are
//! plain JS run in their own `js_runtime::Context` (real quickjs, same
//! engine every page uses), not a new language — the mockup's own sample
//! scripts are already JS (`pane.goto`, `pane.fill`, `pane.click`,
//! `every()`, `on()`).
//!
//! `pane.goto`/`pane.click`/`pane.fill` are all real now (map to
//! `profile::Profile::navigate`/`click`/`fill`, which drive new `NAVIGATE`/
//! `CLICK`/`FILL` commands in `profile-worker`'s stdin protocol — see that
//! binary's own doc comment). Two real scope cuts carried through from
//! there, not this crate's own: only `#id` selectors are supported (this
//! engine has no CSS selector query beyond `dom::Dom::find_by_id`), and
//! `fill` sets `textContent` rather than a real `HTMLInputElement.value`
//! (this engine has no such property at all). `every()` is a plain alias
//! for the already-real `setInterval` (js-runtime's `timers` module)
//! rather than a new primitive. `on`/`emit` and `cron` are real,
//! host-driven event/schedule primitives — see `events` and `cron`
//! modules.
//!
//! No longer ephemeral-only: [`AutomationEngine::rebind`] lets a
//! long-lived host (`apps/shell`'s GUI loop) keep one engine alive across
//! however many frames it wants, re-pointing `pane(...)` at whatever
//! panes are currently live each frame without losing registered
//! `every`/`on`/cron callbacks or JS-side state — closes the crate's own
//! former "ephemeral per run" gap for callers that want it; a caller that
//! just wants one script's top-level code to run once (e.g. a one-shot
//! "auto login" scoped to a single pane) still can via a fresh, dropped-
//! immediately `AutomationEngine`, same as before.
mod cron;
mod events;
mod pane;

pub use cron::CronError;

use js_runtime::{Context, EvalError, Runtime};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// One running automation script: its own JS context, the set of panes
/// (named, *shared* `profile::Profile` handles — see below) it's allowed
/// to control, and the cron/event state `tick()` drives.
pub struct AutomationEngine<'rt> {
    ctx: Context<'rt>,
    // Boxed so the heap address stays valid for the raw pointer stashed via
    // `JS_SetContextOpaque` even though `AutomationEngine` itself may move
    // (e.g. returned out of `new`) — same convention as js-runtime's own
    // `Context::_host_state`.
    //
    // `Rc<RefCell<Profile>>`, not a borrowed `&mut` — a real host
    // (`apps/shell`) needs to keep displaying/polling the same profile
    // while a script also drives it, and (for a long-lived engine kept
    // alive across GUI frames — see [`rebind`](Self::rebind)) needs to
    // hand this engine a *fresh* set of panes every frame without holding
    // a borrow of its own pane list open the whole time. Shared ownership
    // with runtime-checked mutable access is the safe way to let both
    // sides reach the same `Profile` without a self-referential struct or
    // raw pointers into a `Vec` that might reallocate between frames.
    panes: Box<HashMap<String, Rc<RefCell<profile::Profile>>>>,
}

// A plain alias, not a wrapper — so `every` keeps `setInterval`'s real
// argument order, `(callback, delayMs)`. The mockup's own sample scripts
// never show `every()` called with both arguments together, so this is a
// real design call: pick JS's existing convention rather than inventing a
// `(delayMs, callback)` order that would silently misfire (a number where
// `JS_Call` expects a function throws, not a clear error) if a script ever
// assumed the other one.
const PRELUDE: &str = r#"globalThis.every = setInterval;"#;

impl<'rt> AutomationEngine<'rt> {
    /// `panes` are the profiles this script is allowed to name via
    /// `pane("name")` — deliberately explicit rather than "every profile in
    /// the workspace": a script's blast radius should be whatever the host
    /// (e.g. the Settings > Automation "script sandbox" scope the spec
    /// flags as a gap on line 260) grants it, not everything running. A
    /// host with a `workspace::WorkspaceManager` (`apps/shell`) would build
    /// this from the active workspace's profile ids, keeping only the ones
    /// it actually has a live `Profile` for.
    pub fn new(runtime: &'rt Runtime, panes: HashMap<String, Rc<RefCell<profile::Profile>>>) -> Self {
        let ctx = Context::new(runtime);
        let mut panes = Box::new(panes);
        unsafe {
            pane::register(ctx.as_raw(), panes.as_mut() as *mut HashMap<String, Rc<RefCell<profile::Profile>>>);
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

    /// Replaces which real profiles `pane("name")` resolves to, in place —
    /// the primitive a long-lived host (`apps/shell`'s GUI loop, ticking
    /// this same engine every frame so a script's `every`/`on`/`cron`
    /// callbacks actually keep firing, not just registering once and never
    /// running — see `apps/shell/src/main.rs`'s `tick_automation_engine`)
    /// needs to keep pane membership current as panes are added/closed/
    /// moved between workspaces, *without* dropping and recreating the
    /// engine (which would lose every registered `every`/`on`/cron
    /// callback and any JS-side state a script built up).
    ///
    /// Replaces the `HashMap`'s *contents*, not the `Box` itself — the
    /// native `pane` binding's opaque pointer (set once in [`new`](Self::new))
    /// points at the `Box`'s stable heap address, which this must not
    /// move.
    pub fn rebind(&mut self, panes: HashMap<String, Rc<RefCell<profile::Profile>>>) {
        *self.panes = panes;
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

impl Drop for AutomationEngine<'_> {
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
