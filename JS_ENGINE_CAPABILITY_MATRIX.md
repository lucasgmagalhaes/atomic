# JavaScript engine — capability matrix

Last reviewed: 2026-08-24. This is an implementation roadmap for Nimble's embedded web engine, not a claim of full browser compatibility. Percentages are weighted estimates of useful, interoperable behaviour in each area; a checked item is implemented and covered by code or integration tests.

## Overall progress: 46%

| Area | Progress | Current state |
| --- | ---: | --- |
| ECMAScript runtime | 75% | QuickJS-NG provides language execution; host integration remains partial. |
| DOM / HTML APIs | 38% | Core tree, lookup, mutation, selectors and events exist. |
| CSS / layout | 48% | Parsing, cascade, block/inline/flex layout and hit testing exist. |
| Paint / compositing | 42% | GPU rectangles, glyphs, images, clipping and scrolling exist. |
| Browser APIs | 35% | Timers, fetch/XHR, storage, media primitives and a few device APIs exist. |
| Navigation / document lifecycle | 25% | Navigate/reload and frame loop exist; lifecycle is largely absent. |
| Standards / compatibility | 12% | No Web Platform Test conformance target yet. |

## 1. JavaScript runtime and host

**Done (75%)**

- [x] `Runtime`, `Context`, synchronous `eval` and exception conversion.
- [x] ES language execution through QuickJS-NG: classes, modules supported by the upstream runtime, promises, async functions, typed arrays, `Map`, `Set`, regex and standard built-ins.
- [x] Per-context DOM, storage and native-object state; per-runtime class registration.
- [x] Promise-job draining and host timer/event pumping.
- [x] Resource limits on selectors, DOM string inputs, event listeners, bubbling and nested dispatch.
- [x] `crypto.getRandomValues`.

**Needed**

- [ ] ES-module loader/resolver, import maps, dynamic `import()` and module cache.
- [ ] Script loading modes: parser-blocking, `defer`, `async`, module scripts and CSP-aware loading.
- [ ] Interrupt/time budget for untrusted script execution and memory quotas per document.
- [ ] Source maps, debugger protocol, console and structured error reporting.
- [ ] Worker runtime: `Worker`, `SharedWorker`, `MessageChannel`, structured clone and transferable objects.

## 2. DOM classes, properties and functions

### Implemented surface (38%)

| Class/interface | Implemented properties | Implemented functions |
| --- | --- | --- |
| `Document` | `body`, `documentElement`, `activeElement`, `cookie` | `getElementById`, `createElement`, `createTextNode`, `querySelector`, `querySelectorAll` |
| Generic `Node` / element wrapper | `textContent`, `id`, `className`, `parentNode`, `firstChild`, `lastChild`, `previousSibling`, `nextSibling`, `children`, `nodeType`, `nodeName`, `value` | `appendChild`, `insertBefore`, `removeChild`, `remove`, `getAttribute`, `setAttribute`, `removeAttribute`, `querySelector`, `querySelectorAll`, `matches`, `closest`, `focus`, `blur` |
| `EventTarget` | — | `addEventListener`, `removeEventListener`, `dispatchEvent` |
| `Event` | `type`, `target`, `currentTarget`, `bubbles`, `cancelable`, `defaultPrevented` | constructor `new Event(type, options)`, `preventDefault`, `stopPropagation` |
| Form-like nodes | `value`, focus state | native click, input/change/focus/blur dispatch pathways |

### Required DOM roadmap

- [ ] Interface hierarchy and correct prototypes: `Node`, `Element`, `HTMLElement`, `HTMLInputElement`, `HTMLButtonElement`, `HTMLAnchorElement`, `HTMLImageElement`, `HTMLCanvasElement`, etc.
- [ ] `Document`: `head`, `readyState`, `URL`, `baseURI`, `title`, `forms`, `images`, `links`, `scripts`, `getElementsBy*`, `createDocumentFragment`, `createComment`, `importNode`, `adoptNode`.
- [ ] `Node`: `childNodes` live `NodeList`, `ownerDocument`, `isConnected`, `contains`, `cloneNode`, `replaceChild`, `replaceWith`, `before`, `after`, `prepend`, `append`, `normalize`, `compareDocumentPosition`.
- [ ] `Element`: `classList`/`DOMTokenList`, `dataset`, `attributes`/`NamedNodeMap`, `tagName`, `localName`, `namespaceURI`, `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `getBoundingClientRect`, `scroll*`, `client*`, `offset*`.
- [ ] Text/comment/document-fragment nodes and correct `nodeValue` semantics.
- [ ] Collections: live `HTMLCollection`, static `NodeList`, iterable DOM collections.
- [ ] `DOMParser`, `XMLSerializer`, templates, shadow DOM, custom elements and mutation observers.
- [ ] HTML parser integration for scripts/styles: do not render their source; model parser insertion modes and document.write policy.
- [ ] Form controls: checked/selected/disabled/name/type/validity, form submission/reset, labels, selection APIs and `FormData`.

## 3. Events and input

**Done (55%)**

- [x] Stable element identity across lookups.
- [x] Ordered listeners, listener removal, listener count cap and exception isolation.
- [x] Event construction and option flags.
- [x] Target phase and bounded bubbling; cancellation and propagation stop.
- [x] Native profile-worker dispatch for click, keyboard input, focus, blur, input and change.

**Needed**

- [ ] Capturing phase, `once`, `passive`, `signal`, listener objects and full event path/composed semantics.
- [ ] Event subclasses: `MouseEvent`, `PointerEvent`, `KeyboardEvent`, `InputEvent`, `FocusEvent`, `SubmitEvent`, `CustomEvent`, `WheelEvent`, `TouchEvent`, `DragEvent`.
- [ ] Pointer capture, hover/enter/leave, double-click, context menu, drag-and-drop, clipboard events and IME/composition.
- [ ] Default actions: links, buttons, forms, focus navigation, selection, scrolling and cancellation rules.

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

## 5. Paint, canvas and compositing

**Done (42%)**

- [x] GPU background/border rectangle pass.
- [x] CPU glyph rasterization/compositing and decoded image compositing.
- [x] Scroll-offset paint, clip regions, display lists and shared-memory frame publication.
- [x] `Canvas2D` implementation scaffold.

**Needed**

- [ ] Correct paint-order stacking contexts and retained compositing layers.
- [ ] Canvas API completion: paths, transforms, text metrics, image data, gradients, patterns, `toBlob`/`toDataURL`, OffscreenCanvas and WebGL/WebGPU policy.
- [ ] Invalidation regions, damage tracking, frame scheduling and vsync integration.
- [ ] Selection/caret, focus rings, text decorations, SVG and printing/PDF output.

## 6. Browser and Web API classes

| API family | Done | Still needed |
| --- | --- | --- |
| Network | `fetch`, basic `XMLHttpRequest`, proxy/DNS plumbing | `Request`, `Response`, `Headers`, streaming bodies, abort, CORS, cache, redirects/cookies policy, WebSocket, EventSource |
| Timers | `setTimeout`, `setInterval`, `requestAnimationFrame`, cancellation | microtask/checkpoint semantics, idle callbacks, visibility/background throttling policy |
| URL | host URL handling in navigation/resources | `URL`, `URLSearchParams`, origin model, base URL and full relative-resolution exposure |
| Storage | `localStorage`, `sessionStorage`, cookies, IndexedDB primitives | quotas, transactions/indexes/cursors, storage events, Cache Storage, service workers |
| Files | `Blob`, `File`, object URLs | `FileReader`, drag/drop files, streams, filesystem access policy |
| Media | Web Audio offline graph primitives | `AudioContext`, media elements, `MediaSource`, WebRTC, permissions/device selection |
| Device/UI | clipboard, notifications, performance | geolocation, permissions, screen, history, location, fullscreen, dialogs, popups |

## 7. Navigation, security and lifecycle

**Done (25%)**

- [x] Navigate, reload, error-page fallback and per-host storage roots.
- [x] Resource fetching for HTML, CSS and images; profile worker runs document scripts in source order.
- [x] Bounded DOM/event inputs and safe typed host bindings.

**Needed**

- [ ] `window`, `globalThis` browser aliases, `location`, `history`, `navigator`, `screen` and viewport APIs.
- [ ] Same-origin policy, opaque origins, CORS, mixed-content policy, referrer policy and secure cookie attributes.
- [ ] CSP enforcement, sandboxed documents/iframes, permissions policy and trusted types strategy.
- [ ] Lifecycle: `DOMContentLoaded`, `load`, `beforeunload`, `pagehide`, visibility/focus lifecycle and bfcache policy.
- [ ] Multiple browsing contexts: iframe/frame tree, popup policy and cross-window messaging.
- [ ] Request cancellation, navigation races and atomic document replacement.

## 8. Quality gates and measurable milestones

1. **DOM usability — 55% overall**: add `classList`, `dataset`, `innerHTML`, `NodeList`/collections, element-specific prototypes and capture-phase events.
2. **Application compatibility — 65% overall**: `window`/location/history, fetch request-response objects, CSSOM, form semantics, layout measurement and default actions.
3. **Browser correctness — 75% overall**: origin/CORS/CSP/lifecycle/iframe model plus script loading modes and incremental rendering.
4. **Platform readiness — 85% overall**: broad Web Platform Tests, accessibility tree, devtools diagnostics, quotas/timeouts and performance profiling.

## Next implementation order

1. ~~`DOMTokenList` via `element.classList`~~ — done: `add`/`remove`/`contains` (already landed) plus `toggle`/`replace`/`value` getter-setter/`length`/real iteration now real too (`crates/js-runtime/src/dom_bindings.rs`, `sync_class_list`). Stable per-`NodeId` object identity kept (`CLASS_LIST_OBJECTS` cache), refreshed from the live `class` attribute on every mutating call and on every `element.classList` getter hit so a direct `setAttribute("class", ...)` bypass is still picked up on next access. Iteration is real `Symbol.iterator`/`forEach`/etc, not a hand-rolled protocol: the classList object's prototype is set to the real `Array.prototype` (`array_prototype`, via `JS_SetPrototype`) and indexed properties (`0..length`) are kept in sync — `Array.prototype`'s own iterator is generic over any array-like `this`, so this is genuinely spec-shaped, not array-typed (`Array.isArray(classList)` is still `false`, matching real `DOMTokenList`). No `quickjs-sys` binding for well-known symbols was needed or added. `item(index)` isn't a separate method — falls out of the same indexed properties.
2. ~~`dataset` and `attributes`~~ — done. `element.dataset` (`sync_dataset`/`node_dataset_get`) reflects every live `data-*` attribute under its camelCase key (`data-user-id` -> `dataset.userId`), stable per-node identity, resynced on every access; read-side only (`el.dataset.foo = "x"` sets a plain JS property, doesn't write back — real per-key reflection needs a `Proxy`, not exposed by this crate's `quickjs-sys` bindings, same documented scope-down `localStorage`'s missing bracket access already has). `element.attributes` (`sync_attributes`/`node_attributes_get`) is a stable, live, iterable `{name, value}` collection — inherits `Array.prototype` (same `array_prototype` trick `classList` uses) for real `Symbol.iterator`/`length`, plus `getNamedItem(name)`; entries are snapshot objects, not live `Attr` nodes (mutating `.value` on an entry doesn't write back — same read-reflects-live, write-doesn't scope as `dataset`). Next: element-specific properties for form controls and anchors.
3. `innerHTML`/`outerHTML` backed by the existing HTML parser, with bounded input and listener/node-cache invalidation.
4. Capture phase and event listener options; then `CustomEvent`, keyboard and pointer event classes.
5. `window`, `location`, `history`, lifecycle events and navigation cancellation.
6. CSSOM + layout measurement; then hide non-renderable head/style/script source and add incremental invalidation.
7. Origin/security model before expanding cross-origin networking, frames or storage scopes.

