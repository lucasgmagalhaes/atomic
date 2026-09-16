//! `on(name, callback)` — the host-driven half of the mockup's automation
//! surface (watchdog reconnect, claim-idle triggers, ...). Not a DOM event —
//! `AutomationEngine`'s context has no DOM at all, so there's nothing to
//! collide with a page's own `dispatchEvent`.
//!
//! State lives in a plain `Rc<EventState>` owned by `AutomationEngine`, same
//! reasoning `cron.rs`'s own doc gives for dropping the pre-pivot
//! thread-local-keyed-by-context-pointer registry.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rquickjs::{Ctx, Function, Persistent, Result as JsResult};

#[derive(Default)]
pub struct EventState {
    listeners: RefCell<HashMap<String, Vec<Persistent<Function<'static>>>>>,
}

impl EventState {
    /// Same reasoning as `cron::CronState::clear` — must run while `ctx` is
    /// still alive, before `AutomationEngine`'s `Context`/`Runtime` free.
    pub fn clear(&self, ctx: &Ctx<'_>) {
        for (_, callbacks) in self.listeners.borrow_mut().drain() {
            for callback in callbacks {
                if let Ok(f) = callback.restore(ctx) {
                    drop(f);
                }
            }
        }
    }

    /// Calls every listener registered for `name`, in registration order.
    /// Listeners aren't removed — there's no `off()` yet, matching this
    /// pass's "wiring, not full coverage" scope (see this crate's
    /// top-level doc).
    pub fn emit(&self, ctx: &Ctx<'_>, name: &str) {
        let callbacks = self
            .listeners
            .borrow()
            .get(name)
            .cloned()
            .unwrap_or_default();
        for callback in callbacks {
            if let Ok(f) = callback.restore(ctx) {
                let _: JsResult<()> = f.call(());
            }
        }
    }
}

/// Named `'js` for the same reason `cron::make_cron_fn` documents: `Ctx`
/// and `Function` must share exactly one lifetime for `Persistent::save`.
fn make_on_fn<'js>(ctx: &Ctx<'js>, state: Rc<EventState>) -> JsResult<Function<'js>> {
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, name: String, callback: Function<'js>| -> JsResult<()> {
            let persisted = Persistent::save(&ctx, callback);
            state
                .listeners
                .borrow_mut()
                .entry(name)
                .or_default()
                .push(persisted);
            Ok(())
        },
    )
}

/// Registers the `on(name, callback)` global, closing over `state`.
pub(crate) fn register(ctx: &Ctx<'_>, state: Rc<EventState>) -> JsResult<()> {
    let on_fn = make_on_fn(ctx, state)?;
    ctx.globals().set("on", on_fn)?;
    Ok(())
}
