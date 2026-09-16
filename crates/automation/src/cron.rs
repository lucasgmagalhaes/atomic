//! `cron(expression, callback)` — the "cron triggers" the mockup's
//! automation editor exposes. Real minute-granularity 5-field cron
//! (`minute hour day month weekday`), `*` or one exact value per field —
//! no ranges/lists/steps (`1-5`, `*/15`, `1,3,5`), no seconds field. `tick`
//! must be called at least once a minute by the host for a due trigger to
//! actually fire — same cooperative-pump deviation `AutomationEngine::tick`
//! documents.
//!
//! State lives in a plain `Rc<CronState>` owned by `AutomationEngine`, not
//! a thread-local registry keyed by a raw `JSContext` pointer (the
//! pre-CEF-pivot version's approach, needed only because raw `quickjs-sys`
//! gives no way to close a native function over Rust state directly) —
//! `rquickjs::Function::new` closures can capture an `Rc` clone directly,
//! so there's nothing to look up by context identity anymore.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use rquickjs::{Ctx, Exception, Function, Persistent, Result as JsResult};

#[derive(Debug, PartialEq)]
pub struct CronError(pub String);

impl std::fmt::Display for CronError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid cron expression: {}", self.0)
    }
}

#[derive(Clone, Copy)]
struct Field(Option<u32>);

impl Field {
    fn parse(s: &str) -> Result<Field, CronError> {
        if s == "*" {
            Ok(Field(None))
        } else {
            s.parse::<u32>()
                .map(Some)
                .map(Field)
                .map_err(|_| CronError(format!("expected \"*\" or a number, got \"{s}\"")))
        }
    }

    fn matches(self, actual: u32) -> bool {
        self.0.map_or(true, |v| v == actual)
    }
}

struct Schedule {
    minute: Field,
    hour: Field,
    day: Field,
    month: Field,
    weekday: Field,
}

impl Schedule {
    fn parse(expr: &str) -> Result<Schedule, CronError> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = parts[..] else {
            return Err(CronError(format!(
                "expected 5 space-separated fields (minute hour day month weekday), got {}",
                parts.len()
            )));
        };
        Ok(Schedule {
            minute: Field::parse(minute)?,
            hour: Field::parse(hour)?,
            day: Field::parse(day)?,
            month: Field::parse(month)?,
            weekday: Field::parse(weekday)?,
        })
    }

    fn matches(&self, now: &Civil) -> bool {
        self.minute.matches(now.minute)
            && self.hour.matches(now.hour)
            && self.day.matches(now.day)
            && self.month.matches(now.month)
            && self.weekday.matches(now.weekday)
    }
}

struct Civil {
    day: u32,
    month: u32,
    hour: u32,
    minute: u32,
    weekday: u32,
    /// Minutes since the Unix epoch — used only to dedupe firing within
    /// the same minute across multiple `tick()` calls, not exposed to JS.
    epoch_minute: i64,
}

/// Howard Hinnant's `civil_from_days`, the inverse of the algorithm
/// `storage::cookies::days_from_civil` already uses for `Expires` parsing —
/// same convention (hand-rolled, no external date crate), just the other
/// direction.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn now_civil() -> Civil {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_minutes = now.as_secs() as i64 / 60;
    let days = total_minutes.div_euclid(24 * 60);
    let minute_of_day = total_minutes.rem_euclid(24 * 60);
    let (_, month, day) = civil_from_days(days);
    // 1970-01-01 was a Thursday: weekday 0 = Sunday, matching cron/`Date`
    // convention.
    let weekday = ((days.rem_euclid(7)) + 4).rem_euclid(7) as u32;
    Civil {
        day,
        month,
        hour: (minute_of_day / 60) as u32,
        minute: (minute_of_day % 60) as u32,
        weekday,
        epoch_minute: total_minutes,
    }
}

struct CronEntry {
    schedule: Schedule,
    callback: Persistent<Function<'static>>,
    last_fired_minute: Option<i64>,
}

#[derive(Default)]
pub struct CronState {
    entries: RefCell<Vec<CronEntry>>,
}

impl CronState {
    /// Restores and immediately drops every persisted callback while `ctx`
    /// is still alive — must run before the owning `Context`/`Runtime`
    /// free, or quickjs's own shutdown asserts on a non-empty GC object
    /// list (see `AutomationEngine`'s `Drop` impl, which calls this).
    /// `Persistent::drop` alone cannot substitute for this: these entries
    /// are also reachable from the live `cron` global closure itself, so
    /// the last `Rc<CronState>` (and therefore this `Vec`) may not drop
    /// until partway through the context's own teardown, too late for a
    /// `JS_FreeValue`-equivalent call to be safe.
    pub fn clear(&self, ctx: &Ctx<'_>) {
        for entry in self.entries.borrow_mut().drain(..) {
            if let Ok(f) = entry.callback.restore(ctx) {
                drop(f);
            }
        }
    }

    /// Fires every registered entry whose schedule matches the current
    /// minute and hasn't already fired this minute.
    pub fn pump(&self, ctx: &Ctx<'_>) {
        let now = now_civil();
        let due: Vec<Persistent<Function<'static>>> = {
            let mut entries = self.entries.borrow_mut();
            entries
                .iter_mut()
                .filter_map(|entry| {
                    if entry.last_fired_minute != Some(now.epoch_minute)
                        && entry.schedule.matches(&now)
                    {
                        entry.last_fired_minute = Some(now.epoch_minute);
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

/// Named `'js` (not `Ctx<'_>`/`Function<'_>` on the closure directly) so
/// both parameters share exactly one lifetime — required for
/// `Persistent::save(&ctx, callback)`, which needs `ctx` and `callback` at
/// the same `'js`; a plain closure elides each parameter's lifetime
/// independently, which `rquickjs`'s invariant `Ctx`/`Function` types then
/// reject.
fn make_cron_fn<'js>(ctx: &Ctx<'js>, state: Rc<CronState>) -> JsResult<Function<'js>> {
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, expr: String, callback: Function<'js>| -> JsResult<()> {
            let schedule = Schedule::parse(&expr)
                .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
            let persisted = Persistent::save(&ctx, callback);
            state.entries.borrow_mut().push(CronEntry {
                schedule,
                callback: persisted,
                last_fired_minute: None,
            });
            Ok(())
        },
    )
}

/// Registers the `cron(expression, callback)` global, closing over `state`.
pub(crate) fn register(ctx: &Ctx<'_>, state: Rc<CronState>) -> JsResult<()> {
    let cron_fn = make_cron_fn(ctx, state)?;
    ctx.globals().set("cron", cron_fn)?;
    Ok(())
}
