//! `every(callback, delayMs)` — a plain interval timer. Not a real
//! `setInterval`/`setTimeout`: `rquickjs`'s intrinsics are ECMAScript only
//! (no host/browser timer APIs), and unlike the pre-CEF-pivot version of
//! this crate (which reused `neutron::js`'s own real `setInterval`), this
//! crate no longer shares an engine with anything that has one. Same
//! cooperative-pump model as `cron`: a registered entry fires from
//! [`TimerState::pump`] once at least `delay_ms` has elapsed since it last
//! fired (or since registration, for the first fire) — a caller
//! (`AutomationEngine::tick`) must call this at least as often as the
//! shortest registered interval for it to feel real-time.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rquickjs::{Ctx, Function, Persistent, Result as JsResult};

struct TimerEntry {
    delay: Duration,
    callback: Persistent<Function<'static>>,
    last_fired: Instant,
}

#[derive(Default)]
pub struct TimerState {
    entries: RefCell<Vec<TimerEntry>>,
}

impl TimerState {
    /// Same reasoning as `cron::CronState::clear` — must run while `ctx` is
    /// still alive, before `AutomationEngine`'s `Context`/`Runtime` free.
    pub fn clear(&self, ctx: &Ctx<'_>) {
        for entry in self.entries.borrow_mut().drain(..) {
            if let Ok(f) = entry.callback.restore(ctx) {
                drop(f);
            }
        }
    }

    pub fn pump(&self, ctx: &Ctx<'_>) {
        let now = Instant::now();
        let due: Vec<Persistent<Function<'static>>> = {
            let mut entries = self.entries.borrow_mut();
            entries
                .iter_mut()
                .filter_map(|entry| {
                    if now.duration_since(entry.last_fired) >= entry.delay {
                        entry.last_fired = now;
                        Some(entry.callback.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };
        for callback in due {
            if let Ok(f) = callback.restore(ctx) {
                let _: JsResult<()> = f.call(());
            }
        }
    }
}

/// Named `'js` for the same reason `cron::make_cron_fn` documents.
fn make_every_fn<'js>(ctx: &Ctx<'js>, state: Rc<TimerState>) -> JsResult<Function<'js>> {
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, callback: Function<'js>, delay_ms: f64| -> JsResult<()> {
            let persisted = Persistent::save(&ctx, callback);
            state.entries.borrow_mut().push(TimerEntry {
                delay: Duration::from_secs_f64((delay_ms.max(0.0)) / 1000.0),
                callback: persisted,
                last_fired: Instant::now(),
            });
            Ok(())
        },
    )
}

/// Registers the `every(callback, delayMs)` global, closing over `state`.
pub(crate) fn register(ctx: &Ctx<'_>, state: Rc<TimerState>) -> JsResult<()> {
    let every_fn = make_every_fn(ctx, state)?;
    ctx.globals().set("every", every_fn)?;
    Ok(())
}
