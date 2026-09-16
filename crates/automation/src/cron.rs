//! `cron(expression, callback)` — the "cron triggers" the mockup's
//! automation editor exposes. Real minute-granularity 5-field cron
//! (`minute hour day month weekday`), `*` or one exact value per field —
//! no ranges/lists/steps (`1-5`, `*/15`, `1,3,5`), no seconds field. `tick`
//! must be called at least once a minute by the host for a due trigger to
//! actually fire — same cooperative-pump deviation as js-runtime's
//! `timers`/`fetch_async` (no real event loop yet, see this crate's
//! `AutomationEngine::tick` doc).
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;
use std::time::{SystemTime, UNIX_EPOCH};

use neutron::quickjs_sys as sys;

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
    // Returned to JS as the `cron()` call's id; not read Rust-side yet —
    // there's no `clearCron(id)` in this pass, matching `on`'s own "no
    // `off()` yet" scope note in `events.rs`.
    #[allow(dead_code)]
    id: u32,
    schedule: Schedule,
    callback: sys::JSValue,
    last_fired_minute: Option<i64>,
}

#[derive(Default)]
struct CronState {
    next_id: u32,
    entries: Vec<CronEntry>,
}

thread_local! {
    static REGISTRIES: RefCell<HashMap<usize, CronState>> = RefCell::new(HashMap::new());
}

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
    let mut len: usize = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, val, false);
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
    let s = String::from_utf8_lossy(bytes).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

unsafe extern "C" fn cron_fn(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return sys::js_undefined();
    }
    let Some(expr) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let schedule = match Schedule::parse(&expr) {
        Ok(s) => s,
        Err(e) => {
            let msg = e.to_string();
            let js_msg =
                sys::JS_NewStringLen(ctx, msg.as_ptr() as *const std::os::raw::c_char, msg.len());
            return sys::JS_Throw(ctx, js_msg);
        }
    };
    let callback = sys::JS_DupValue(ctx, *argv.add(1));
    let id = REGISTRIES.with(|reg| {
        let mut map = reg.borrow_mut();
        let state = map.entry(ctx as usize).or_insert_with(CronState::default);
        state.next_id += 1;
        let id = state.next_id;
        state.entries.push(CronEntry {
            id,
            schedule,
            callback,
            last_fired_minute: None,
        });
        id
    });
    sys::js_float64(id as f64)
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    let name_c = CString::new("cron").unwrap();
    let f = sys::JS_NewCFunction2(ctx, cron_fn, name_c.as_ptr(), 2, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, global, name_c.as_ptr(), f);
    sys::JS_FreeValue(ctx, global);
}

/// Fires every registered `cron()` entry whose schedule matches the current
/// minute and hasn't already fired this minute.
pub(crate) unsafe fn pump(ctx: *mut sys::JSContext) {
    let now = now_civil();
    let due = REGISTRIES.with(|reg| {
        let mut map = reg.borrow_mut();
        let Some(state) = map.get_mut(&(ctx as usize)) else {
            return Vec::new();
        };
        let mut due = Vec::new();
        for entry in state.entries.iter_mut() {
            if entry.last_fired_minute != Some(now.epoch_minute) && entry.schedule.matches(&now) {
                entry.last_fired_minute = Some(now.epoch_minute);
                due.push(sys::JS_DupValue(ctx, entry.callback));
            }
        }
        due
    });
    for callback in due {
        let result = sys::JS_Call(ctx, callback, sys::js_undefined(), 0, std::ptr::null_mut());
        sys::JS_FreeValue(ctx, result);
        sys::JS_FreeValue(ctx, callback);
    }
}

/// Frees every still-registered cron callback for `ctx`. Must run before
/// `JS_FreeContext(ctx)`.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    let Some(state) = REGISTRIES.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) else {
        return;
    };
    for entry in state.entries {
        sys::JS_FreeValue(ctx, entry.callback);
    }
}
