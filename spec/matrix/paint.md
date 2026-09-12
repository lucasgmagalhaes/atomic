# Paint, Canvas & Compositing — Capability Status

## 5. Paint, canvas and compositing

**Done (42%)**

- [x] GPU background/border rectangle pass.
- [x] CPU glyph rasterization/compositing and decoded image compositing.
- [x] Scroll-offset paint, clip regions, display lists and shared-memory frame publication.
- [x] `Canvas2D` implementation scaffold.

**Needed**

- [~] Correct paint-order stacking contexts done (2026-09-12) for the real trigger (`position != Static && z_index.is_some()`) — see `spec/ROADMAP.md` items 24-25. Retained compositing layers still `[ ]`.
- [ ] Canvas API completion: paths, transforms, text metrics, image data, gradients, patterns, `toBlob`/`toDataURL`, OffscreenCanvas and WebGL/WebGPU policy.
- [ ] Invalidation regions, damage tracking, frame scheduling and vsync integration.
- [ ] Selection/caret, focus rings, text decorations, SVG and printing/PDF output.

---

[← back to spec/INDEX.md](../INDEX.md)
