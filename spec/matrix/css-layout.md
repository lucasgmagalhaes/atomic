# HTML, CSS & Layout — Capability Status

## 4. HTML, CSS and layout

**Done (58%)**

- [x] HTML5 parsing via html5ever with implied document structure and recovery.
- [x] CSS lexer/parser, selector matching and cascade.
- [x] Selectors including common compound/combinator, attribute, nth and pseudo-class support represented by the CSS crate.
- [x] Style resolution, media-query viewport evaluation, block layout, inline text layout, flex layout, float/clear, overflow and hit testing.
- [x] Image intrinsic sizing and real decoded image resources.
- [x] Selector-matching index (2026-09-12) — `css::SelectorIndex`, bucketed by id/class/tag/universal, wired into `layout_engine::tree`'s box-tree construction (`ROADMAP.md` P1 item 9).

**Needed**

- [x] CSSOM: `CSSStyleSheet`, `CSSStyleDeclaration`, `getComputedStyle`, `style`, stylesheet mutation and adopted stylesheets. Stale line - already fully real, not a gap: `getComputedStyle(element)` (`crates/js-runtime/src/computed_style.rs`) is a real global returning a snapshot `CSSStyleDeclaration`-shaped object backed by `HostState.computed_styles`, which `profile-worker`'s `Page::render` pushes in wholesale after running `layout-engine`'s real cascade resolver - documented deviation: a point-in-time snapshot, not a live view (calling it again after a mutation + re-render gives an up-to-date answer, but the same returned object doesn't mutate in place). `element.style` (`crates/js-runtime/src/css_style/`) is a real `CSSStyleDeclaration` reflecting the inline `style` attribute both directions, with `setProperty`/`getPropertyValue`/`removeProperty`/`cssText`/named kebab-to-camel accessors for every property `layout-engine`'s cascade resolver understands. `CSSStyleSheet` (`crates/js-runtime/src/cssom_stylesheet.rs`) has real `insertRule`/`deleteRule`/`cssRules`/`document.adoptedStyleSheets`. All covered by real tests (`computed_style_test.rs`, `cssom_stylesheet_test.rs`).
- [ ] Incremental style/layout invalidation and retained layout tree (currently safe full relayout is used).
- [ ] Full formatting contexts: grid, table, multicolumn, ruby, list markers, replaced-element rules and fragmentation.
- [~] Full position model: sticky/fixed edge cases and containing blocks still `[ ]`. Stacking contexts and z-index are real (2026-09-12) — `ComputedStyle::z_index`, `render::display_list`'s paint-order sort — see `spec/ROADMAP.md` items 24-25 for the exact scope cut. `transform` is real but 2D-translate-only (2026-09-12, `ROADMAP.md` item 26) — `rotate`/`scale`/`skew`/`matrix` parse but don't paint (no rotation/scale in this crate's paint primitives).
- [ ] Fonts: `@font-face`, fallback, shaping, kerning, bidi, line breaking and font loading API.
- [ ] CSS animations, transitions, filters, gradients, masks, blend modes and container queries.
- [ ] Accessibility tree and semantics derived from the DOM/layout tree.

---

[← back to spec/INDEX.md](../INDEX.md)
