//! One (re)loadable "page": the parsed DOM's root `<html>` element, its
//! own resolved stylesheet, real fetched images, and the JS context
//! mutating it — plus layout/render/hit-test, all sharing one cached box
//! tree so they never disagree with each other.
//!
//! Split into `load.rs` (`Page::load`) and `render.rs` (`layout`/
//! `content_height`/`render`/`hit_test_at`) — this file keeps the struct
//! definitions and `DEMO_SCRIPT`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use css::Stylesheet;
use dom::NodeId;
use image_decode::DecodedImage;
use js_runtime::Context;
use layout_engine::LayoutBox;

mod load;
mod render;

/// Runs only against the built-in demo page (never a real navigated
/// page, which has no `#counter` element for it to find): increments a
/// counter and writes it into `#counter`'s text every 50ms via
/// `setInterval`, and separately bumps a `requestAnimationFrame`-driven
/// counter every tick, proving both timer kinds are actually pumped by
/// the host loop rather than just accepted and ignored.
/// Also demonstrates real persisted `localStorage`/`document.cookie` on
/// the demo page's own `#counter` text: `visits` increments in real
/// `localStorage` every time this script runs (i.e. every `RELOAD`), and
/// `document.cookie` gets a real cookie set - both readable proof (via
/// the rendered text a `profile` test can read back from actual painted
/// pixels) that `Context::with_storage`'s wiring reaches a real page, not
/// just something `js-runtime`'s own unit tests exercise in isolation.
pub(crate) const DEMO_SCRIPT: &str = r#"
var tickCount = 0;
var rafCount = 0;
var visits = parseInt(localStorage.getItem('visits') || '0', 10) + 1;
localStorage.setItem('visits', String(visits));
document.cookie = 'visited=true';
function onTick() {
    tickCount++;
    document.getElementById('counter').textContent = 'tick ' + tickCount + ' raf ' + rafCount + ' visits ' + visits;
    setTimeout(onTick, 50);
}
function onFrame() {
    rafCount++;
    requestAnimationFrame(onFrame);
}
setTimeout(onTick, 50);
requestAnimationFrame(onFrame);
"#;

/// One (re)loadable "page": the parsed DOM's root `<html>` element, its
/// own resolved stylesheet (base + whatever `<style>`/`<link>` sources it
/// contributed), and the JS context mutating it. Rebuilt from scratch on
/// every navigation/reload, same as a real navigation resetting a page's
/// JS state (and its stylesheet — a page's CSS doesn't survive its own
/// reload any more than its JS does, matching real navigation).
pub(crate) struct Page<'rt> {
    pub(crate) ctx: Context<'rt>,
    html_el: NodeId,
    sheet: Stylesheet,
    /// Real fetched/decoded `<img>` images, keyed by the `<img>` element's
    /// own `NodeId` — see `load_images`. Fixed for this `Page`'s lifetime
    /// (fetched once at load time, same "a page's resources don't change
    /// underneath it without a fresh navigation" convention its
    /// stylesheet already follows).
    images: HashMap<NodeId, Rc<DecodedImage>>,
    /// Real incremental layout invalidation: the last computed tree, plus
    /// the exact inputs it was computed from. `layout()` reuses it
    /// (a clone — far cheaper than re-running `build_box_tree_with_viewport`
    /// + text shaping/flex resolution + `layout_block` from scratch) when
    /// none of `width`/`dom.layout_version()`/`dom.style_version()`/
    /// `adopted_stylesheet_version()` have changed since. `RefCell` because
    /// `layout()` is called from both `&self` methods (`content_height`,
    /// `hit_test_at`) and `&mut self` ones (`render`) — a shared cache
    /// needs interior mutability either way. `None` before the first
    /// `layout()` call.
    ///
    /// This is also this engine's real "computed style cache"
    /// (`ROADMAP.md` P1 item 10, `architecture/performance.md` §14.2's
    /// `(node, stylesheet_version, parent_style_version) -> ComputedStyle`
    /// shape) — every `LayoutBox` embeds its own resolved `ComputedStyle`,
    /// so reusing the cached tree *is* reusing cached computed styles,
    /// not a separate structure. `getComputedStyle`
    /// (`js_runtime::computed_style`) reads from whatever `render()` last
    /// pushed via `collect_computed_styles`, which comes straight from
    /// this cache.
    layout_cache: RefCell<Option<LayoutCache>>,
    /// Real paint damage tracking (`ROADMAP.md` P1 item 13): the last
    /// composited frame, plus the exact inputs it was computed from.
    /// `render()` reuses `pixels.clone()` on a cache hit instead of
    /// re-walking the display list and re-running the GPU/CPU compositing
    /// passes — see `PaintCache`'s own doc for why this reuses
    /// `LayoutCache`'s key rather than `dom::DirtyFlags::PAINT`.
    paint_cache: RefCell<Option<PaintCache>>,
    /// Real per-compositing-layer paint cache (`ROADMAP.md` item 28),
    /// keyed by each layer root's own `NodeId` — unlike `paint_cache`
    /// (one whole-frame entry, invalidated by *any* page-wide change),
    /// each entry here survives across frames independently, reused
    /// whenever `render()` finds no `dom::StyleInvalidation` whose target
    /// falls under that layer's own subtree (see `render.rs`'s own doc on
    /// `find_layer_roots`/`Dom::contains` wiring). Entries whose `NodeId`
    /// no longer resolves to a current layer root are pruned each
    /// recompute so this can't grow unboundedly across navigations.
    layers: RefCell<HashMap<NodeId, LayerCacheEntry>>,
}

/// One [`render::Layer`]'s cached pixels plus the `scroll_top` they were
/// painted at — `render::LayerCacheKey` itself has no `scroll_top` field
/// (a layer's own painted pixels embed the scroll shift, same as the
/// whole-frame `PaintCache` already accounts for), so this wrapper keeps
/// it alongside the layer without touching `render::Layer`'s own,
/// already-landed shape.
struct LayerCacheEntry {
    layer: ::render::Layer,
    scroll_top: f64,
}

struct LayoutCache {
    width: u32,
    height: u32,
    /// `dom::Dom::style_version()` at the time this tree was built — a
    /// `:hover`/`:focus` change (`Dom::set_hovered`/`focus`/`blur`) bumps
    /// this without bumping `layout_version` (see `dom::Dom::style_version`'s
    /// own doc), so it must be a separate cache key: without it, a click
    /// that only focuses an element (nothing else layout-affecting) would
    /// keep returning the stale pre-focus tree — a real bug this field
    /// closes, proven by `crates/profile/tests/profile_test.rs`'s
    /// `clicking_to_focus_an_input_updates_its_real_computed_focus_style`.
    style_ver: u64,
    layout_ver: u64,
    adopted_version: u64,
    tree: LayoutBox,
}

/// Caches `render()`'s composited pixels the same way `LayoutCache` caches
/// the box tree they were painted from — same `width`/`height`/`layout_ver`/
/// `style_ver`/`adopted_version` key (a paint result can only differ if the
/// layout tree it was painted from could differ), plus `scroll_top`, the
/// one paint-affecting input `LayoutCache` doesn't need (scrolling doesn't
/// change layout, only which vertical slice of it is visible).
///
/// Deliberately **not** keyed on `dom::DirtyFlags::PAINT`/`drain_dirty()`:
/// `Dom::focus`/`hover::set_hovered` bump `style_version()` without ever
/// setting `DirtyFlags::PAINT` (same gap `LayoutCache::style_ver`'s own doc
/// already documents), so a `DirtyFlags`-only cache would silently freeze a
/// stale frame across a `:hover`/`:focus` style change. Reusing
/// `LayoutCache`'s already-proven-correct key sidesteps that gap instead of
/// reintroducing it. `DirtyFlags::PAINT` itself is untouched by this pass —
/// reserved for a future, more precise per-region damage pass (see
/// `spec/architecture/performance.md` §18), not repurposed here.
struct PaintCache {
    width: u32,
    height: u32,
    scroll_top: f64,
    layout_ver: u64,
    style_ver: u64,
    adopted_version: u64,
    pixels: Vec<u8>,
}
