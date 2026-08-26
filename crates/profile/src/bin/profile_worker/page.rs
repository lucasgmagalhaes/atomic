//! One (re)loadable "page": the parsed DOM's root `<html>` element, its
//! own resolved stylesheet, real fetched images, and the JS context
//! mutating it — plus layout/render/hit-test, all sharing one cached box
//! tree so they never disagree with each other.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use css::{parse_stylesheet, Stylesheet};
use dom::NodeId;
use image_decode::DecodedImage;
use js_runtime::{Context, Runtime};
use layout_engine::{
    apply_image_sizes, build_box_tree_with_viewport, layout_block, LayoutBox, PositionedGlyph,
};
use render::{
    build_display_list, build_glyph_list, build_image_list, composite_glyphs, composite_images,
    ClipRect, ClippedGlyph, GpuRenderer, ImageQuad, Rect,
};

use crate::document_load::LoadedDocument;
use crate::layout_snapshot::{collect_computed_styles, collect_layout_rects};
use crate::page_source::{
    build_stylesheet, collect_meta_csp_policies, collect_script_sources, load_images,
};

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
    /// none of `width`/`dom.mutation_count()`/`adopted_stylesheet_version()`
    /// have changed since. `RefCell` because `layout()` is called from
    /// both `&self` methods (`content_height`, `hit_test_at`) and `&mut
    /// self` ones (`render`) — a shared cache needs interior mutability
    /// either way. `None` before the first `layout()` call.
    layout_cache: RefCell<Option<LayoutCache>>,
}

struct LayoutCache {
    width: u32,
    dom_mutations: u64,
    adopted_version: u64,
    tree: LayoutBox,
}

impl<'rt> Page<'rt> {
    /// Builds a page from `doc` (the fetched HTML plus whatever CSP
    /// policies its response delivered - see [`LoadedDocument`]).
    /// `run_demo_script` should only be `true`
    /// for the built-in demo page - a real fetched page has no
    /// `#counter` element for `DEMO_SCRIPT` to find (which now uses real
    /// `localStorage`/`document.cookie` itself - see the const's doc).
    /// `document.cookie`/`localStorage`/`sessionStorage`/`indexedDB` are
    /// wired to real per-`storage_host` storage under `storage_root` (via
    /// `js_runtime::Context::with_storage`) whenever `storage_host` is
    /// `Some` - `None` gets a plain `Context::with_dom` instead. A
    /// storage-open failure (rare - a permissions problem, a full disk)
    /// degrades to `with_dom` rather than failing the whole page load.
    /// Every CSP policy `doc` carried (response headers) plus every
    /// `<meta http-equiv="Content-Security-Policy">` tag in the parsed
    /// HTML is enforced before the first script runs.
    pub(crate) fn load(
        runtime: &'rt Runtime,
        doc: &LoadedDocument,
        run_demo_script: bool,
        base_url: Option<&str>,
        storage_host: Option<&str>,
        viewport_width: f64,
        storage_root: &std::path::Path,
        proxy: Option<&net::ProxyConfig>,
        dns_server: Option<std::net::SocketAddr>,
    ) -> Self {
        let html = &doc.html;
        let (dom, html_el) = html::parse_to_html_element(html);
        let sheet = build_stylesheet(
            &dom,
            html_el,
            base_url,
            viewport_width,
            storage_root,
            proxy,
            dns_server,
        );
        let images = load_images(&dom, html_el, base_url, storage_root, proxy, dns_server);
        let mut scripts = Vec::new();
        collect_script_sources(&dom, html_el, &mut scripts);

        let mut ctx = match storage_host {
            Some(host) => {
                match Context::with_storage(runtime, dom, host, storage_root.join(host)) {
                    Ok(ctx) => ctx,
                    // `with_storage` already consumed `dom` by the time it can
                    // fail (a rare I/O error opening the storage files) - it
                    // has to be re-parsed from `html` rather than reused,
                    // acceptable for a path this unlikely to hit in practice.
                    Err(_) => Context::with_dom(runtime, html::parse_to_html_element(html).0),
                }
            }
            None => Context::with_dom(runtime, dom),
        };
        if let Some(url) = base_url {
            ctx.set_url(url);
        }
        // Real CSP delivery, before any script runs - matching a real
        // browser, where a page's policy is fully in effect by the time its
        // first script executes. Two delivery channels, enforced together:
        // the document response's own `Content-Security-Policy` headers
        // (extracted in `resolve_document`, repeated headers kept as
        // separate policies) and every `<meta http-equiv>` tag the parsed
        // DOM turned out to carry. Each is appended via `add_csp_policy`
        // rather than joined into one string - real CSP policies intersect
        // rather than merge (see `js_runtime::csp`'s module docs).
        for policy in &doc.csp_policies {
            ctx.add_csp_policy(policy);
        }
        let mut meta_policies = Vec::new();
        collect_meta_csp_policies(
            ctx.dom()
                .expect("Page::load always builds its context over a dom"),
            html_el,
            &mut meta_policies,
        );
        for policy in meta_policies {
            ctx.add_csp_policy(&policy);
        }
        if run_demo_script {
            let _ = ctx.eval(DEMO_SCRIPT, "<profile-worker demo>");
        }
        // Real page scripts, in real document order - a later script
        // failing (a real JS exception) doesn't stop earlier ones from
        // having already run, matching how a real browser keeps executing
        // a page after one `<script>` throws (each gets its own top-level
        // try - this engine still has no `window.onerror`/console to
        // report it to, so a failure is silent here, same "no error
        // surface for a page's own script" scope every other page-script
        // path in this worker already has).
        for (index, script) in scripts.iter().enumerate() {
            let _ = ctx.eval(script, &format!("<script {index}>"));
        }
        // Real `DOMContentLoaded`/`load` lifecycle timing: fired once the
        // document is parsed and every page script has run, matching a
        // real browser's own ordering (see `Context::dispatch_lifecycle_events`'s
        // doc for the scope this crate cuts relative to the full spec).
        ctx.dispatch_lifecycle_events();
        Page {
            ctx,
            html_el,
            sheet,
            images,
            layout_cache: RefCell::new(None),
        }
    }

    /// Builds and lays out this page's real box tree against `width` —
    /// shared by `render` and `hit_test_at` so they always agree on
    /// exactly the same box positions/sizes (previously each rebuilt its
    /// own tree independently, which would have silently disagreed once
    /// `<img>` intrinsic sizing entered the picture - `apply_image_sizes`
    /// must run after box-tree construction and before layout, so both
    /// callers need it applied identically). `None` only if the parse/
    /// box-tree-construction step itself fails.
    fn layout(&self, width: u32) -> Option<LayoutBox> {
        let dom = self.ctx.dom()?;
        let dom_mutations = dom.mutation_count();
        // Real `document.adoptedStyleSheets` mutation support (see
        // `js_runtime::cssom_stylesheet`): a script's `insertRule`/
        // `deleteRule` doesn't bump `dom.mutation_count()` (it touches a
        // `CSSStyleSheet`, not the DOM), so this cheap version key (no
        // rule text touched) still invalidates the cache below.
        let adopted_version = self.ctx.adopted_stylesheet_version();

        if let Some(cached) = self.layout_cache.borrow().as_ref() {
            if cached.width == width
                && cached.dom_mutations == dom_mutations
                && cached.adopted_version == adopted_version
            {
                return Some(cached.tree.clone());
            }
        }

        // Only paid for on an actual cache miss: the real rule text, for
        // merging the page's own base stylesheet (built once at `load()`
        // time) with whatever's currently adopted - cloning rather than
        // mutating `self.sheet` in place keeps the base untouched if a
        // later layout has nothing adopted anymore.
        let adopted_text = self.ctx.adopted_stylesheet_text();
        let sheet = if adopted_text.is_empty() {
            std::borrow::Cow::Borrowed(&self.sheet)
        } else {
            let mut merged = self.sheet.clone();
            merged.rules.extend(parse_stylesheet(&adopted_text).rules);
            std::borrow::Cow::Owned(merged)
        };
        let mut tree = build_box_tree_with_viewport(dom, self.html_el, &sheet, width as f64)?;
        apply_image_sizes(dom, &mut tree, &self.images);
        layout_block(&mut tree, width as f64, 0.0, 0.0);

        *self.layout_cache.borrow_mut() = Some(LayoutCache {
            width,
            dom_mutations,
            adopted_version,
            tree: tree.clone(),
        });
        Some(tree)
    }

    /// This page's real total content height at `width` — the root box's
    /// own laid-out height, which already includes every descendant
    /// (block layout stacks children downward with no clamping to any
    /// viewport). Used to clamp `SCROLL`'s offset to real content, not an
    /// arbitrary range. `0.0` if layout itself fails (same "nothing to
    /// scroll" outcome as a page shorter than its viewport).
    pub(crate) fn content_height(&self, width: u32) -> f64 {
        self.layout(width)
            .map(|tree| tree.dimensions.height)
            .unwrap_or(0.0)
    }

    /// Re-layouts and re-rasterizes from the DOM's *current* state (which
    /// may have just been mutated by a JS timer callback) against an
    /// already-initialized `renderer` — creating a `GpuRenderer` opens a
    /// real GPU device/adapter, expensive enough that doing it every tick
    /// would itself become the vsync loop's bottleneck instead of the
    /// fixed frame interval. Composites real decoded `<img>` pixels
    /// (`self.images`, via `build_image_list`/`composite_images`) between
    /// the background-rect pass and the text pass — closest to real paint
    /// order (background, then replaced content, then inline text) this
    /// worker's existing two-pass (GPU rects, then CPU glyphs) pipeline
    /// can give without a bigger repaint-ordering rework.
    ///
    /// `scroll_top` is a real viewport scroll offset (see the `SCROLL`
    /// command's own doc): every painted rect/image/glyph is shifted up
    /// by this many pixels before compositing, so what's actually visible
    /// in `[0, height)` is the document's `[scroll_top, scroll_top +
    /// height)` slice — layout itself is unaffected (boxes keep their
    /// real absolute document-space positions; only the paint step
    /// windows into them), which is also why `build_display_list`/
    /// `build_image_list`/`build_glyph_list` don't need a `scroll_top`
    /// parameter of their own. `render`/`gpu`/`text`'s own compositors
    /// already clip anything outside `[0, height)` silently (real
    /// clipping, not new for this), so a shifted rect above or below the
    /// viewport is simply not drawn - no separate clip step needed here.
    /// Real `overflow: hidden`/`auto`/`scroll` clipping (per-element, see
    /// `layout_engine::Overflow`) rides along the same shift: an
    /// `ImageQuad`/`ClippedGlyph`'s own `clip` region is in the same
    /// absolute document space as everything else, so it needs the same
    /// `-offset`/`-scroll_top` shift applied as the quad/glyph it clips,
    /// or a scrolled page would clip against a stale, unscrolled region.
    pub(crate) fn render(
        &mut self,
        renderer: &GpuRenderer,
        width: u32,
        height: u32,
        scroll_top: f64,
    ) -> Vec<u8> {
        let tree = self
            .layout(width)
            .expect("parsed HTML always produces a box");
        self.ctx.set_layout_rects(collect_layout_rects(&tree));
        self.ctx.set_computed_styles(collect_computed_styles(&tree));
        let offset = scroll_top as f32;
        let shift_clip =
            |clip: Option<ClipRect>, dy: f32| clip.map(|c| ClipRect { y: c.y - dy, ..c });

        let rects: Vec<Rect> = build_display_list(&tree)
            .into_iter()
            .map(|r| Rect {
                y: r.y - offset,
                ..r
            })
            .collect();
        let images: Vec<ImageQuad> = build_image_list(&tree)
            .into_iter()
            .map(|q| ImageQuad {
                y: q.y - offset,
                clip: shift_clip(q.clip, offset),
                ..q
            })
            .collect();
        let glyphs: Vec<ClippedGlyph> = build_glyph_list(&tree)
            .into_iter()
            .map(|g| ClippedGlyph {
                glyph: PositionedGlyph {
                    y: g.glyph.y - scroll_top as i32,
                    ..g.glyph
                },
                clip: shift_clip(g.clip, offset),
                opacity: g.opacity,
            })
            .collect();

        let mut pixels = renderer.render_to_rgba(&rects, width, height, [0.08, 0.09, 0.13, 1.0]);
        composite_images(&mut pixels, width, height, &images);
        composite_glyphs(&mut pixels, width, height, &glyphs);
        pixels
    }

    /// Real coordinate-to-DOM-node hit test — lays out the same box tree
    /// `render` does (via `self.layout`, against `width`, the pane's own
    /// real frame width, so the caller's `(x, y)` must already be in that
    /// same on-screen pixel space) and walks it via
    /// `layout_engine::hit_test`. `scroll_top` converts `y` from that
    /// on-screen space back to the document's own absolute space (the
    /// inverse of the shift `render` applies when painting) before
    /// hit-testing, so a click against a scrolled page still lands on the
    /// real element under the cursor, not whatever was there before any
    /// `SCROLL`. `None` covers both "point is outside every box" and "the
    /// parse/layout step itself failed" — a caller can't distinguish
    /// those from this return value alone, matching this method's only
    /// real use (`CLICK_AT`, where both cases report the same "nothing
    /// there" outcome anyway).
    pub(crate) fn hit_test_at(
        &self,
        width: u32,
        x: f64,
        y: f64,
        scroll_top: f64,
    ) -> Option<NodeId> {
        let tree = self.layout(width)?;
        layout_engine::hit_test(&tree, x, y + scroll_top)
    }
}
