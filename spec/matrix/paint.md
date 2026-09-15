# Paint, Canvas & Compositing — Capability Status

## 5. Paint, canvas and compositing

**Done (46%)**

- [x] GPU background/border rectangle pass.
- [x] CPU glyph rasterization/compositing and decoded image compositing.
- [x] Scroll-offset paint, clip regions, display lists and shared-memory frame publication.
- [x] Real `HTMLCanvasElement.getContext('2d')` JS wiring (2026-09-14) — the `render::Canvas2D` scaffold (`fillRect`/`clearRect`/`fillStyle`) is now actually reachable from page JavaScript (`js-runtime`'s new `canvas_bindings` module), not just a Rust-only orphan. `getContext(id)` only recognizes `"2d"`; the returned `CanvasRenderingContext2D` has just those three real members. Real per-canvas persistence: the backing `Canvas2D` lives in `HostState::canvases`, keyed by the canvas element's own `NodeId`, so drawn pixels/`fillStyle` survive across separate `getContext('2d')` calls on the same element — the one real spec deviation is that each call still returns a *new* JS wrapper object (`===` across two calls is `false`), since this crate has no per-`(ctx, NodeId)` cached-`JSValue` registry for this yet (the pattern `class_list`/`dataset`/`css_style` already use elsewhere would be the natural next step). `fillStyle` only parses/formats `#rgb`/`#rrggbb` hex (no `rgb()`/`hsl()`/named keywords — neither `render::Canvas2D` nor any publicly exposed `layout_engine` API had a general CSS-color-string parser to reuse). Real page compositing: `profile-worker`'s `Page::render` reads every active canvas's current pixels fresh every frame (`Context::canvas_snapshots`) and feeds them into a new `layout_engine::apply_canvas_snapshots`, which reuses the *existing*, already-tested `<img>` compositing pipeline (`render::build_image_list`/`composite_images`) — a canvas paints like any other replaced element, no new paint primitive. A canvas's own `width`/`height` HTML attributes (default `300`/`150`) size only its backing pixel buffer, read once at the first `getContext('2d')` call — not its layout box size, which ordinary CSS (or this crate's own block-level Auto-stretch default) always decides; a page wanting a specific on-screen size still needs real CSS `width`/`height`. Since this engine has no cheap way to detect whether a canvas's drawn content actually changed since the last frame, a page with any active 2D canvas context bypasses `PaintCache`/per-layer caching on *every* frame while that canvas exists (same shape this pass's CSS-transitions work already introduced for `TransitionStates`). Tested at three levels: `js-runtime/tests/canvas_test.rs` (7 tests: real context object, unsupported `contextId` returns `null`, hex `fillStyle` round-trip incl. 3-digit expansion, shared backing across repeated `getContext` calls), `layout-engine/tests/canvas_snapshot_test.rs` (5 tests), and `profile/tests/canvas2d_test.rs` (2 real end-to-end pixel tests through `Profile::spawn` + navigate + `latest_frame()`).
- [x] `linear-gradient` background paint (2026-09-14) — real per-vertex GPU color interpolation for a 2-stop `background`/`background-image: linear-gradient(...)` (`render::gpu::shader::rect_to_vertices`), mathematically exact for the 2-stop case, no new shader code. See `spec/matrix/css-layout.md`'s own entry for the CSS-parsing side. `radial-gradient`/`conic-gradient` and `Canvas2D`'s own separate gradient API (`createLinearGradient`/`createRadialGradient`) are still `[ ]` — see the "Needed" line below.

**Needed**

- [~] Correct paint-order stacking contexts done (2026-09-12) for the real trigger (`position != Static && z_index.is_some()`) — see `spec/ROADMAP.md` items 24-25. Retained compositing layers still `[ ]`.
- [ ] Canvas API completion: paths, strokes, text, `drawImage`, transforms, text metrics, `getImageData`/`putImageData` (real in Rust via `Canvas2D::get_image_data`, not yet bound to JS), gradients, patterns, `toBlob`/`toDataURL`, OffscreenCanvas and WebGL/WebGPU policy.
- [ ] Invalidation regions, damage tracking, frame scheduling and vsync integration.
- [ ] Selection/caret, focus rings, text decorations, SVG and printing/PDF output.

---

[← back to spec/INDEX.md](../INDEX.md)
