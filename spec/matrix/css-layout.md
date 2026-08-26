# HTML, CSS & Layout — Capability Status

## 4. HTML, CSS and layout

**Done (48%)**

- [x] HTML5 parsing via html5ever with implied document structure and recovery.
- [x] CSS lexer/parser, selector matching and cascade.
- [x] Selectors including common compound/combinator, attribute, nth and pseudo-class support represented by the CSS crate.
- [x] Style resolution, media-query viewport evaluation, block layout, inline text layout, flex layout, float/clear, overflow and hit testing.
- [x] Image intrinsic sizing and real decoded image resources.

**Needed**

- [ ] CSSOM: `CSSStyleSheet`, `CSSStyleDeclaration`, `getComputedStyle`, `style`, stylesheet mutation and adopted stylesheets.
- [ ] Incremental style/layout invalidation and retained layout tree (currently safe full relayout is used).
- [ ] Full formatting contexts: grid, table, multicolumn, ruby, list markers, replaced-element rules and fragmentation.
- [ ] Full position model: sticky/fixed edge cases, containing blocks, stacking contexts, z-index and transforms.
- [ ] Fonts: `@font-face`, fallback, shaping, kerning, bidi, line breaking and font loading API.
- [ ] CSS animations, transitions, transforms, filters, gradients, masks, blend modes and container queries.
- [ ] Accessibility tree and semantics derived from the DOM/layout tree.

---

[← back to spec/INDEX.md](../INDEX.md)
