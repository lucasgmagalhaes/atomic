//! `Page::layout`/`content_height`/`render`/`hit_test_at` — split out
//! from `page.rs`.

use std::collections::HashSet;

use css::parse_stylesheet;
use dom::NodeId;
use layout_engine::style::{BorderStyle, Color};
use layout_engine::{
    apply_image_sizes, build_box_tree_with_viewport, find_layer_roots, layout_block, LayoutBox,
    PositionedGlyph,
};
use render::{
    build_display_list, build_glyph_list, build_image_list, composite_glyphs, composite_images,
    composite_layer_onto, ClipRect, ClippedGlyph, GpuRenderer, ImageQuad, Layer, LayerCacheKey,
    Rect,
};

use crate::layout_snapshot::{
    collect_computed_styles, collect_layout_rects, collect_scroll_extents,
};

use super::{LayerCacheEntry, LayoutCache, Page, PaintCache};

/// Empties a layer root's own subtree out of a (cloned) base tree so the
/// base pass's own `build_display_list`/`build_image_list`/
/// `build_glyph_list` paint it zero times — its content instead paints
/// once, inside its own layer canvas (see `Page::render`'s own doc). Kept
/// as a plain `NodeId` match rather than a tree-position walk: every
/// layer root is a real element box (`establishes_stacking_context` only
/// matches an element's own resolved style), never an anonymous merged
/// inline-run box, so the `NodeId` ambiguity `LayoutBox::node`'s own doc
/// warns about for those doesn't apply here.
fn prune_layer_roots(box_: &mut LayoutBox, layer_root_ids: &HashSet<NodeId>) {
    if layer_root_ids.contains(&box_.node) {
        box_.children.clear();
        box_.text = None;
        box_.inline_spans = None;
        box_.glyphs.clear();
        box_.image = None;
        box_.style.background_color = Color::TRANSPARENT;
        box_.style.border_style = BorderStyle::None;
        box_.style.box_shadow = None;
        return;
    }
    for child in &mut box_.children {
        prune_layer_roots(child, layer_root_ids);
    }
}

impl<'rt> Page<'rt> {
    /// Builds and lays out this page's real box tree against `width`/
    /// `height` (the latter feeding `@media (min-height: ...)`/
    /// `(max-height: ...)` resolution, same role `width` already plays for
    /// `(min-width: ...)` — see `css::MediaQuery::matches`) — shared by
    /// `render` and `hit_test_at` so they always agree on exactly the same
    /// box positions/sizes (previously each rebuilt its own tree
    /// independently, which would have silently disagreed once `<img>`
    /// intrinsic sizing entered the picture - `apply_image_sizes` must run
    /// after box-tree construction and before layout, so both callers need
    /// it applied identically). `None` only if the parse/box-tree-
    /// construction step itself fails.
    fn layout(&self, width: u32, height: u32) -> Option<LayoutBox> {
        let dom = self.ctx.dom()?;
        // `layout_version` bumps only when a layout-relevant mutation
        // occurs (LAYOUT dirty flag set), so it's the precise cache key
        // for "has layout been dirtied since last rebuild?" — cheaper and
        // more precise than `mutation_count()`, which bumps on every
        // mutation including attribute-only changes that don't affect layout.
        let layout_ver = dom.layout_version();
        // `:hover`/`:focus` change which CSS rules match without ever
        // setting the LAYOUT dirty flag (see `dom::Dom::style_version`'s
        // own doc) - `layout_ver` alone can't see that, so this is a
        // separate, required cache key (see `LayoutCache::style_ver`'s
        // own doc for the real bug this closes).
        let style_ver = dom.style_version();
        // Real `document.adoptedStyleSheets` mutation support (see
        // `js_runtime::cssom_stylesheet`): a script's `insertRule`/
        // `deleteRule` doesn't bump `dom.mutation_count()` (it touches a
        // `CSSStyleSheet`, not the DOM), so this cheap version key (no
        // rule text touched) still invalidates the cache below.
        let adopted_version = self.ctx.adopted_stylesheet_version();

        if let Some(cached) = self.layout_cache.borrow().as_ref() {
            if cached.width == width
                && cached.height == height
                && cached.layout_ver == layout_ver
                && cached.style_ver == style_ver
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
        let mut tree =
            build_box_tree_with_viewport(dom, self.html_el, &sheet, width as f64, height as f64)?;
        apply_image_sizes(dom, &mut tree, &self.images);
        layout_block(&mut tree, width as f64, 0.0, 0.0);

        *self.layout_cache.borrow_mut() = Some(LayoutCache {
            width,
            height,
            layout_ver,
            style_ver,
            adopted_version,
            tree: tree.clone(),
        });
        Some(tree)
    }

    /// This page's real total content height at `width`/`height` — the
    /// root box's own laid-out height, which already includes every
    /// descendant (block layout stacks children downward with no clamping
    /// to any viewport). Used to clamp `SCROLL`'s offset to real content,
    /// not an arbitrary range. `0.0` if layout itself fails (same "nothing
    /// to scroll" outcome as a page shorter than its viewport).
    pub(crate) fn content_height(&self, width: u32, height: u32) -> f64 {
        self.layout(width, height)
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
        // Real paint damage tracking (`ROADMAP.md` P1 item 13): same key
        // `LayoutCache` uses (a paint result can only differ if the layout
        // tree it was painted from could) plus `scroll_top`, the one
        // paint-affecting input layout doesn't need — see `PaintCache`'s
        // own doc for why this key is used instead of `dom::DirtyFlags::PAINT`.
        let dom = self
            .ctx
            .dom()
            .expect("Page::load always builds a context over a dom");
        let layout_ver = dom.layout_version();
        let style_ver = dom.style_version();
        let adopted_version = self.ctx.adopted_stylesheet_version();
        if let Some(cached) = self.paint_cache.borrow().as_ref() {
            if cached.width == width
                && cached.height == height
                && cached.scroll_top == scroll_top
                && cached.layout_ver == layout_ver
                && cached.style_ver == style_ver
                && cached.adopted_version == adopted_version
            {
                return cached.pixels.clone();
            }
        }

        let tree = self
            .layout(width, height)
            .expect("parsed HTML always produces a box");
        self.ctx.set_layout_rects(collect_layout_rects(&tree));
        self.ctx.set_computed_styles(collect_computed_styles(&tree));
        self.ctx.set_scroll_extents(collect_scroll_extents(&tree));
        let offset = scroll_top as f32;
        let shift_clip =
            |clip: Option<ClipRect>, dy: f32| clip.map(|c| ClipRect { y: c.y - dy, ..c });
        // Real `position: fixed` (`layout_engine::Position::Fixed`): a
        // primitive `render`'s own display-list builders tagged `fixed`
        // (see `Rect::fixed`'s own doc) skips this page-scroll shift
        // entirely, keeping it glued to the viewport while the rest of
        // the page scrolls underneath - no separate "fixed layer" needed,
        // since this is applied per-primitive at the exact point every
        // other primitive already gets shifted.
        let shift_y = |y: f32, fixed: bool| if fixed { y } else { y - offset };
        let shift_clip_if = |clip: Option<ClipRect>, fixed: bool| {
            if fixed {
                clip
            } else {
                shift_clip(clip, offset)
            }
        };

        // Real compositing layers (`ROADMAP.md` item 28): boxes that
        // establish a stacking context (`layout_engine::find_layer_roots`)
        // paint into their own transparent canvas, cached independently -
        // reused across frames unless Stage 2's `dom::StyleInvalidation`
        // queue names a target under that layer's own subtree (drained
        // once per recompute below, checked via `Dom::contains`). Without
        // this, every layer would share the same coarse whole-page key and
        // invalidate together on any change - see `layer.rs`'s own doc.
        let layer_roots = find_layer_roots(&tree);
        let layer_root_ids: HashSet<NodeId> = layer_roots.iter().map(|b| b.node).collect();
        self.layers
            .borrow_mut()
            .retain(|id, _| layer_root_ids.contains(id));

        let dom_mut = self
            .ctx
            .dom_mut()
            .expect("Page::load always builds a context over a dom");
        let invalidations = dom_mut.drain_style_invalidations();
        let dom = self
            .ctx
            .dom()
            .expect("Page::load always builds a context over a dom");
        let mut dirty_layers: HashSet<NodeId> = HashSet::new();
        for invalidation in &invalidations {
            for root in &layer_roots {
                if root.node == invalidation.root || dom.contains(root.node, invalidation.root) {
                    dirty_layers.insert(root.node);
                }
            }
        }

        let layer_key = LayerCacheKey {
            width,
            height,
            layout_ver,
            style_ver,
            adopted_version,
        };

        let mut base_tree = tree.clone();
        prune_layer_roots(&mut base_tree, &layer_root_ids);

        let rects: Vec<Rect> = build_display_list(&base_tree)
            .into_iter()
            .map(|r| Rect {
                y: shift_y(r.y, r.fixed),
                ..r
            })
            .collect();
        let images: Vec<ImageQuad> = build_image_list(&base_tree)
            .into_iter()
            .map(|q| ImageQuad {
                y: shift_y(q.y, q.fixed),
                clip: shift_clip_if(q.clip, q.fixed),
                ..q
            })
            .collect();
        let glyphs: Vec<ClippedGlyph> = build_glyph_list(&base_tree)
            .into_iter()
            .map(|g| ClippedGlyph {
                glyph: PositionedGlyph {
                    y: if g.fixed {
                        g.glyph.y
                    } else {
                        g.glyph.y - scroll_top as i32
                    },
                    ..g.glyph
                },
                clip: shift_clip_if(g.clip, g.fixed),
                opacity: g.opacity,
                fixed: g.fixed,
            })
            .collect();

        let mut pixels = renderer.render_to_rgba(&rects, width, height, [0.08, 0.09, 0.13, 1.0]);
        composite_images(&mut pixels, width, height, &images);
        composite_glyphs(&mut pixels, width, height, &glyphs);

        // Nearest-ancestor-flattened stacking order: paint in ascending
        // `z-index` (ties keep `find_layer_roots`' own pre-order/document
        // order) - real nested-layer-within-layer compositing is a
        // documented scope cut (see `stacking.rs`'s own doc).
        let mut sorted_roots = layer_roots;
        sorted_roots.sort_by_key(|b| b.style.z_index.unwrap_or(0));

        for layer_root in sorted_roots {
            let dirty = dirty_layers.contains(&layer_root.node);
            let reuse = !dirty
                && self
                    .layers
                    .borrow()
                    .get(&layer_root.node)
                    .is_some_and(|entry| {
                        entry.layer.is_valid(layer_key) && entry.scroll_top == scroll_top
                    });

            let layer_pixels = if reuse {
                self.layers
                    .borrow()
                    .get(&layer_root.node)
                    .expect("just checked present and valid above")
                    .layer
                    .pixels
                    .clone()
            } else {
                let layer_rects: Vec<Rect> = build_display_list(layer_root)
                    .into_iter()
                    .map(|r| Rect {
                        y: shift_y(r.y, r.fixed),
                        ..r
                    })
                    .collect();
                let layer_images: Vec<ImageQuad> = build_image_list(layer_root)
                    .into_iter()
                    .map(|q| ImageQuad {
                        y: shift_y(q.y, q.fixed),
                        clip: shift_clip_if(q.clip, q.fixed),
                        ..q
                    })
                    .collect();
                let layer_glyphs: Vec<ClippedGlyph> = build_glyph_list(layer_root)
                    .into_iter()
                    .map(|g| ClippedGlyph {
                        glyph: PositionedGlyph {
                            y: if g.fixed {
                                g.glyph.y
                            } else {
                                g.glyph.y - scroll_top as i32
                            },
                            ..g.glyph
                        },
                        clip: shift_clip_if(g.clip, g.fixed),
                        opacity: g.opacity,
                        fixed: g.fixed,
                    })
                    .collect();

                let mut layer_canvas =
                    renderer.render_to_rgba(&layer_rects, width, height, [0.0, 0.0, 0.0, 0.0]);
                composite_images(&mut layer_canvas, width, height, &layer_images);
                composite_glyphs(&mut layer_canvas, width, height, &layer_glyphs);

                let mut layer = Layer::new(layer_root.node);
                layer.store(layer_key, layer_canvas.clone());
                self.layers
                    .borrow_mut()
                    .insert(layer_root.node, LayerCacheEntry { layer, scroll_top });
                layer_canvas
            };

            composite_layer_onto(&mut pixels, &layer_pixels, width, height);
        }

        *self.paint_cache.borrow_mut() = Some(PaintCache {
            width,
            height,
            scroll_top,
            layout_ver,
            style_ver,
            adopted_version,
            pixels: pixels.clone(),
        });
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
        height: u32,
        x: f64,
        y: f64,
        scroll_top: f64,
    ) -> Option<NodeId> {
        let tree = self.layout(width, height)?;
        layout_engine::hit_test(&tree, x, y + scroll_top)
    }
}
