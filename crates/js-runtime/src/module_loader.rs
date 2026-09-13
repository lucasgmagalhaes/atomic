//! ES Module resource loader + cache (`ROADMAP.md` item 19;
//! `.claude/plans/architecture-p5-foundations.plan.md` Stage 4).
//!
//! Scope cut, deliberate: this lands the resolver + fetch-and-cache
//! primitive only — real module semantics (import/export binding,
//! linking, a module namespace object, `import()` returning a real
//! `Promise` resolving to that namespace, circular-dependency handling)
//! stay item 19's own remaining work, built on top of this rather than
//! this stage inventing a parallel mechanism
//! (`architecture/overview.md`'s "architecture before APIs" rule). What's
//! real here: a specifier resolves against the importing module's own
//! URL via `url::Url::join` — the exact primitive `URL`'s own constructor
//! (`url_bindings::url_constructor`) already uses for its `base`
//! argument — and a module's source text is fetched at most once per
//! `Context` and cached by resolved URL, the same "compute once, key by
//! an identity, reuse on a hit" shape `LayoutCache`/`PaintCache` already
//! use for layout/paint.
//!
//! Background fetch mirrors `fetch_async::fetch_fn`'s own shape (a real
//! OS thread via `net::request`, delivered through [`pump`] on the same
//! "host pumps explicitly, no real event loop yet" cadence every other
//! async-ish primitive in this crate already has) rather than a new
//! fetch mechanism.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;

use quickjs_sys as sys;

/// A module fetch's real, observable state. `Loading` until [`pump`]
/// delivers a result; `Ready`/`Failed` are terminal — a caller who wants
/// to reload the same URL as a genuinely fresh fetch isn't supported
/// today (real ES module semantics only ever fetch a given URL once per
/// realm, matching this).
#[derive(Debug, Clone)]
pub enum ModuleStatus {
    Loading,
    Ready(String),
    Failed(String),
}

#[derive(Default)]
struct ModuleCache {
    records: HashMap<String, ModuleStatus>,
    pending: Vec<(String, mpsc::Receiver<Result<net::Response, net::Error>>)>,
}

thread_local! {
    // Keyed by JSContext pointer, same reasoning as `fetch_async::STATE`.
    static CACHES: RefCell<HashMap<usize, ModuleCache>> = RefCell::new(HashMap::new());
}

/// Resolves an `import`/dynamic-`import()` specifier against the
/// importing module's own URL. Real relative/absolute resolution, not a
/// string-concatenation approximation — `None` for an unparseable
/// `base_url` or `specifier` (a bare specifier with no import map —
/// ROADMAP item 19's own remaining work — can't resolve without one,
/// same as a real browser without one configured).
pub(crate) fn resolve_specifier(base_url: &str, specifier: &str) -> Option<String> {
    let base = url::Url::parse(base_url).ok()?;
    base.join(specifier).ok().map(|u| u.to_string())
}

/// Starts a background fetch of `resolved_url`'s source text, or is a
/// real no-op if it's already cached or in flight — the actual point of
/// this being a *cache*, not just a fetch wrapper: a module imported by
/// two different modules in the same page must not trigger two network
/// round trips.
pub(crate) fn load(ctx: *mut sys::JSContext, resolved_url: &str) {
    CACHES.with(|c| {
        let mut map = c.borrow_mut();
        let cache = map.entry(ctx as usize).or_default();
        if cache.records.contains_key(resolved_url) {
            return;
        }
        cache
            .records
            .insert(resolved_url.to_string(), ModuleStatus::Loading);
        let (tx, rx) = mpsc::channel();
        let url = resolved_url.to_string();
        thread::spawn(move || {
            let _ = tx.send(net::request("GET", &url, &[], None));
        });
        cache.pending.push((resolved_url.to_string(), rx));
    });
}

/// Real cache read side — `None` if [`load`] was never called for this
/// URL on this `Context`.
pub(crate) fn status(ctx: *mut sys::JSContext, resolved_url: &str) -> Option<ModuleStatus> {
    CACHES.with(|c| {
        c.borrow()
            .get(&(ctx as usize))
            .and_then(|cache| cache.records.get(resolved_url).cloned())
    })
}

/// Moves every completed background fetch's result into the cache — same
/// "host pumps explicitly" cadence every other async-ish primitive in
/// this crate already has. Returns how many module fetches completed.
pub(crate) fn pump(ctx: *mut sys::JSContext) -> usize {
    let due = CACHES.with(|c| {
        let mut map = c.borrow_mut();
        let Some(cache) = map.get_mut(&(ctx as usize)) else {
            return Vec::new();
        };
        let mut due = Vec::new();
        let mut i = 0;
        while i < cache.pending.len() {
            match cache.pending[i].1.try_recv() {
                Ok(result) => due.push((cache.pending.remove(i).0, result)),
                Err(_) => i += 1,
            }
        }
        due
    });
    if due.is_empty() {
        return 0;
    }
    let delivered = due.len();
    CACHES.with(|c| {
        let mut map = c.borrow_mut();
        let cache = map.entry(ctx as usize).or_default();
        for (url, result) in due {
            let record = match result {
                Ok(response) => {
                    ModuleStatus::Ready(String::from_utf8_lossy(&response.body).into_owned())
                }
                Err(e) => ModuleStatus::Failed(e.to_string()),
            };
            cache.records.insert(url, record);
        }
    });
    delivered
}

/// Frees `ctx`'s module cache — must run before `JS_FreeContext(ctx)`,
/// same ordering requirement every other module's `cleanup(ctx)` already
/// documents (nothing here holds a `JSValue`, so this is just a `HashMap`
/// drop, not a `JS_FreeValue` pass — still done at the same point in
/// `Context::drop` for consistency).
pub(crate) fn cleanup(ctx: *mut sys::JSContext) {
    CACHES.with(|c| {
        c.borrow_mut().remove(&(ctx as usize));
    });
}
