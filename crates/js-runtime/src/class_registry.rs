//! Per-`JSRuntime` QuickJS class registration — replaces an earlier
//! design (one `static ..._CLASS_ID: AtomicU32` per class, shared and
//! reused across *every* `Runtime` in the process) that turned out to
//! be unsound under concurrent `Runtime` construction, not just racy.
//!
//! The original design's reasoning ("class IDs are just table indices,
//! safe to reuse across runtimes since each runtime keeps its own class
//! table") is true for *reuse after the fact*, but concurrent
//! construction breaks the part that reasoning depended on: each
//! `JSRuntime` allocates fresh custom class IDs from its own
//! `js_class_id_alloc` counter, incrementing once per class *this
//! specific runtime* freshly allocates (`JS_NewClassID`'s `*pclass_id ==
//! 0` branch in quickjs.c). Sharing one static value across runtimes
//! means whichever runtime happens to be first to claim a given class
//! kind "wins" that numeric ID for the whole process — but *which*
//! runtime wins differs per class kind under real concurrent execution
//! (Rust's test harness runs each `#[test]` fn on its own thread, and
//! most of this crate's tests each build a fresh `Runtime`). A runtime
//! that loses the race for one class kind never increments its own
//! counter for it; if that same runtime *wins* the race for some other
//! kind shortly after, its counter is now out of step with whatever the
//! "canonical" process-wide numbering implies — a genuine collision
//! between two logically different classes sharing one numeric ID
//! *within a single runtime's own class table*. Confirmed empirically:
//! adding a lock around just the `JS_NewClassID`/`JS_NewClass` call
//! pair (an earlier attempt at this fix) eliminated the immediate
//! memory-corruption crashes but left a reproducible, non-crashing
//! symptom - `Blob.prototype.slice()`'s result sometimes had no `size`/
//! `text`/etc, i.e. the wrong (or no) prototype, under concurrent load
//! but never in isolation.
//!
//! The real fix: never reuse a numeric class ID *value* across
//! different `Runtime`s at all — there is no correctness reason to
//! (JSValues never cross a `Runtime` boundary anywhere in this crate),
//! it was solving a non-problem. Each runtime gets its own, guaranteed-
//! fresh class ID via [`ensure_class`], keyed by `(runtime pointer,
//! class-kind name)` in a process-wide map — the map (not any single
//! numeric value) is what's now shared, and every mutation goes through
//! one `Mutex`. Any function that later needs to validate/read an
//! object's opaque data (`JS_GetOpaque`) must look its runtime's class
//! ID back up via [`class_id_for`] rather than caching a bare number,
//! since the ID is only meaningful relative to one specific `Runtime`.
//! [`cleanup_runtime`] must be called from `Runtime::drop` — Windows'
//! allocator readily reuses a freed `JSRuntime`'s address for the next
//! one, and a stale map entry at that address would make a brand new
//! runtime's `ensure_class` wrongly believe a class is already
//! registered on it.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use quickjs_sys as sys;

type Key = (usize, &'static str);

fn registry() -> &'static Mutex<HashMap<Key, sys::JSClassID>> {
    static REGISTRY: OnceLock<Mutex<HashMap<Key, sys::JSClassID>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns `rt`'s class ID for `kind`, registering a fresh one (via a
/// genuinely fresh `JS_NewClassID` allocation, then `JS_NewClass` with
/// `def`) the first time this `(rt, kind)` pair is seen. `def` is only
/// read on that first call — later calls for the same pair return the
/// cached ID without touching `JS_NewClass` again (e.g. `Blob` and
/// `File` intentionally share one class/kind, so `File`'s call after
/// `Blob`'s on the same runtime is a pure cache hit).
pub(crate) unsafe fn ensure_class(rt: *mut sys::JSRuntime, kind: &'static str, def: &sys::JSClassDef) -> sys::JSClassID {
    let mut map = registry().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let key = (rt as usize, kind);
    if let Some(&id) = map.get(&key) {
        return id;
    }
    let mut fresh: sys::JSClassID = 0;
    let id = sys::JS_NewClassID(rt, &mut fresh);
    sys::JS_NewClass(rt, id, def);
    map.insert(key, id);
    id
}

/// Looks up `rt`'s already-registered class ID for `kind`, for
/// `JS_GetOpaque`/similar validation in functions that only have
/// `this_val`/`rt` (not the `JSClassDef` [`ensure_class`] needs).
/// Returns `0` (`JS_INVALID_CLASS_ID`, never a real user class ID) if
/// `kind` was never registered for this runtime — safely fails whatever
/// `JS_GetOpaque` check follows, rather than panicking.
pub(crate) fn class_id_for(rt: *mut sys::JSRuntime, kind: &'static str) -> sys::JSClassID {
    registry().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&(rt as usize, kind)).copied().unwrap_or(0)
}

/// Removes every entry for `rt` — must be called from `Runtime::drop`
/// before the runtime's memory can be reused by a new one (see module
/// docs).
pub(crate) fn cleanup_runtime(rt: *mut sys::JSRuntime) {
    registry().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).retain(|(rt_ptr, _), _| *rt_ptr != rt as usize);
}
