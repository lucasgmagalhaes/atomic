//! User automation scripts: the `pane`/`every`/`on`/cron surface the mockup
//! shows (login automático, claim idle, watchdog reconexão). Scripts are
//! plain JS run in their own `rquickjs` context — this crate's own embedded
//! scripting engine, independent of whatever engine renders page content
//! (CEF/Chromium, post-pivot — see `CLAUDE.md`'s Engine pivot). Automation
//! scripts never touch a page's DOM directly; they drive `profile::Profile`
//! handles over the same `NAVIGATE`/`CLICK`/`FILL` protocol the GUI itself
//! uses, so which engine renders the page is irrelevant to this crate.
//!
//! `pane.goto`/`pane.click`/`pane.fill` map to `profile::Profile::navigate`/
//! `click`/`fill`, which drive `NAVIGATE`/`CLICK`/`FILL` commands in
//! `profile-worker`'s stdin protocol — see that binary's own doc comment.
//! Two real scope cuts carried through from there, not this crate's own:
//! only `#id` selectors are supported, and `fill` sets `textContent` rather
//! than a real `HTMLInputElement.value`. `every()` is a plain alias for the
//! already-real `setInterval`. `on`/`emit` and `cron` are real, host-driven
//! event/schedule primitives — see `events` and `cron` modules.
//!
//! No longer ephemeral-only: [`AutomationEngine::rebind`] lets a
//! long-lived host (`crates/atomic`'s GUI loop) keep one engine alive across
//! however many frames it wants, re-pointing `pane(...)` at whatever panes
//! are currently live each frame without losing registered `every`/`on`/cron
//! callbacks or JS-side state — closes the crate's own former "ephemeral per
//! run" gap for callers that want it; a caller that just wants one script's
//! top-level code to run once (e.g. a one-shot "auto login" scoped to a
//! single pane) still can via a fresh, dropped-immediately
//! `AutomationEngine`, same as before.
mod cron;
mod events;
mod pane;
mod timers;

pub use cron::CronError;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rquickjs::{Context, Runtime as JsRuntime};

/// One running automation script: its own JS context, the set of panes
/// (named, *shared* `profile::Profile` handles — see below) it's allowed
/// to control, and the cron/event state `tick()` drives.
pub struct AutomationEngine {
    // Field order is drop order (top to bottom) — every `Persistent<Function>`
    // held by `cron_state`/`event_state`/`timer_state` must be dropped
    // *before* `ctx`/`_runtime` free the quickjs context/runtime they were
    // persisted against, or quickjs's own shutdown asserts on a non-empty
    // GC object list (a real, reproducible crash this ordering fixes, not
    // a hypothetical one — see the fields' own doc comment below).
    //
    // `Rc<RefCell<Profile>>`, not a borrowed `&mut` — a real host
    // (`crates/atomic`) needs to keep displaying/polling the same profile
    // while a script also drives it, and (for a long-lived engine kept
    // alive across GUI frames — see [`rebind`](Self::rebind)) needs to
    // hand this engine a *fresh* set of panes every frame without holding
    // a borrow of its own pane list open the whole time.
    panes: Rc<RefCell<pane::Panes>>,
    cron_state: Rc<cron::CronState>,
    event_state: Rc<events::EventState>,
    timer_state: Rc<timers::TimerState>,
    ctx: Context,
    _runtime: JsRuntime,
}

impl AutomationEngine {
    /// `panes` are the profiles this script is allowed to name via
    /// `pane("name")` — deliberately explicit rather than "every profile in
    /// the workspace": a script's blast radius should be whatever the host
    /// grants it. A host with a `workspace::WorkspaceManager`
    /// (`crates/atomic`) would build this from the active workspace's
    /// profile ids, keeping only the ones it actually has a live `Profile`
    /// for.
    pub fn new(panes: HashMap<String, Rc<RefCell<profile::Profile>>>) -> Self {
        let runtime = JsRuntime::new().expect("quickjs runtime allocation must not fail");
        let ctx = Context::full(&runtime).expect("quickjs context allocation must not fail");
        let panes = Rc::new(RefCell::new(panes));
        let cron_state = Rc::new(cron::CronState::default());
        let event_state = Rc::new(events::EventState::default());
        let timer_state = Rc::new(timers::TimerState::default());

        ctx.with(|ctx| {
            pane::register(&ctx, panes.clone()).expect("pane bindings must register");
            events::register(&ctx, event_state.clone()).expect("event bindings must register");
            cron::register(&ctx, cron_state.clone()).expect("cron bindings must register");
            timers::register(&ctx, timer_state.clone()).expect("timer bindings must register");
        });

        AutomationEngine {
            _runtime: runtime,
            ctx,
            panes,
            cron_state,
            event_state,
            timer_state,
        }
    }

    /// Replaces which real profiles `pane("name")` resolves to, in place —
    /// the primitive a long-lived host (`crates/atomic`'s GUI loop, ticking
    /// this same engine every frame so a script's `every`/`on`/`cron`
    /// callbacks actually keep firing) needs to keep pane membership
    /// current as panes are added/closed/moved between workspaces,
    /// *without* dropping and recreating the engine (which would lose
    /// every registered `every`/`on`/cron callback and any JS-side state a
    /// script built up).
    pub fn rebind(&mut self, panes: HashMap<String, Rc<RefCell<profile::Profile>>>) {
        *self.panes.borrow_mut() = panes;
    }

    /// Runs a user script's top-level code once (registers its
    /// `every`/`on`/cron callbacks; doesn't fire any of them yet).
    pub fn run(&self, source: &str, _filename: &str) -> Result<String, String> {
        self.ctx.with(|ctx| {
            ctx.eval::<rquickjs::Coerced<String>, _>(source)
                .map(|coerced| coerced.0)
                .map_err(|e| e.to_string())
        })
    }

    /// One cooperative tick: pumps due timers/`requestAnimationFrame`
    /// (`every` included, since it's just `setInterval`) and due cron
    /// triggers. No real host-driven event loop yet — a caller (the GUI's
    /// per-frame loop, or tests) must call this explicitly.
    pub fn tick(&self) {
        // `execute_pending_job` locks the same runtime `ctx.with` locks —
        // called from outside any `.with()` closure, never nested inside
        // one, or rquickjs's internal borrow guard panics ("RefCell
        // already borrowed"), a real reproducible crash this ordering
        // fixes (see `run_pending_timers`-equivalent gotchas this
        // workspace already documents for the pre-pivot engine).
        while self._runtime.execute_pending_job().unwrap_or(false) {}
        self.ctx.with(|ctx| {
            self.timer_state.pump(&ctx);
            self.cron_state.pump(&ctx);
        });
    }

    /// Fires every `on(name, ...)` listener registered for `name` — the
    /// host's hook for events a script can't observe on its own (e.g. a
    /// watchdog noticing a pane went unresponsive).
    pub fn emit(&self, name: &str) {
        self.ctx.with(|ctx| self.event_state.emit(&ctx, name));
    }

    pub fn pane_names(&self) -> Vec<String> {
        self.panes.borrow().keys().cloned().collect()
    }
}

impl Drop for AutomationEngine {
    /// Explicitly frees every `Persistent<Function>` this engine holds
    /// while `self.ctx` is still alive — see `cron::CronState::clear`'s own
    /// doc for why relying on `Rc`/`Persistent`'s ordinary drop glue alone
    /// (even with the struct's fields declared in the right order) isn't
    /// enough: these callbacks are also reachable from the live `cron`/
    /// `on`/`every` global closures the context itself owns, so the last
    /// `Rc` to each state can drop partway through the context's own
    /// teardown instead of before it.
    fn drop(&mut self) {
        self.ctx.with(|ctx| {
            self.cron_state.clear(&ctx);
            self.event_state.clear(&ctx);
            self.timer_state.clear(&ctx);
        });
    }
}
