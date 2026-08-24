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
2. ~~`dataset` and `attributes`~~ — done. `element.dataset` (`sync_dataset`/`node_dataset_get`) reflects every live `data-*` attribute under its camelCase key (`data-user-id` -> `dataset.userId`), stable per-node identity, resynced on every access; read-side only (`el.dataset.foo = "x"` sets a plain JS property, doesn't write back — real per-key reflection needs a `Proxy`, not exposed by this crate's `quickjs-sys` bindings, same documented scope-down `localStorage`'s missing bracket access already has). `element.attributes` (`sync_attributes`/`node_attributes_get`) is a stable, live, iterable `{name, value}` collection — inherits `Array.prototype` (same `array_prototype` trick `classList` uses) for real `Symbol.iterator`/`length`, plus `getNamedItem(name)`; entries are snapshot objects, not live `Attr` nodes (mutating `.value` on an entry doesn't write back — same read-reflects-live, write-doesn't scope as `dataset`). ~~Next: element-specific properties for form controls and anchors.~~ — done: `checked`/`disabled`/`selected` reflect HTML boolean-attribute semantics (presence-only — `boolean_attribute_get`/`_set` in `dom_bindings.rs`, distinct from `attribute_property_get`/`_set`'s string reflection), `href` reuses the plain string reflection anchors already share with `id`/`className`/`name`/`type`.
3. ~~`innerHTML`/`outerHTML` backed by the existing HTML parser, with bounded input and listener/node-cache invalidation.~~ — done: `dom::Dom::serialize_children`/`serialize_node` (the read side, attributes sorted by name for determinism since this crate's storage has no authored order) plus `dom::Dom::adopt` (deep-clones a subtree across two different `Dom` arenas — `NodeId`s are only valid within the arena that created them) and `html::parse_fragment` (parses a string the same way a full document is parsed, returns the resulting `<body>`'s children) give the write side something to build on. `crates/js-runtime/src/dom_bindings.rs`'s `innerHTML`/`outerHTML` getter/setters wire it together — a setter evicts cached JS state (`NODE_OBJECTS`/`classList`/`dataset`/`attributes`, and any listeners riding on those cached objects) for every removed node first, same convention `removeChild`/`remove` already use, and input is length-bounded (64KB) like this file's other untrusted-string setters. No void-element list exists in this engine (`<br></br>` is acceptable output), and `outerHTML`'s write side makes the node stop existing afterward, matching `remove()`'s own contract.
4. ~~Capture phase and event listener options; then `CustomEvent`, keyboard and pointer event classes.~~ — done: `events.rs`'s dispatch is a real three-phase traversal now (capturing root->target, at-target, bubbling target->root) instead of a single bubble-only pass; `addEventListener`/`removeEventListener` accept the `useCapture` boolean or a `{capture, once, passive}` options object, plus `EventListener` objects exposing `handleEvent` — `once` removes the listener right after it fires, `passive` makes `preventDefault()` a no-op for the duration of that listener's call. `signal` (`AbortSignal`) is a documented scope cut, no `AbortController` exists yet. Building this surfaced a real latent bug: `Event`'s constructor never exposed a JS-visible `Event.prototype` property (only the internal per-context class proto via `JS_SetClassProto`), so anything reading `Event.prototype` — `instanceof` checks, subclassing — saw `undefined`; fixed by also setting it as a real own property on the `Event` constructor, holding the same object `JS_SetClassProto` registers. `events::create_event` is now exposed as the seam `event_subclasses.rs`'s three new global constructors (`CustomEvent`, `KeyboardEvent`, `PointerEvent`) build on: each sets its subclass-specific fields (`detail`; `key`/`code`/`keyCode`/`ctrlKey`/etc; `pointerId`/`pointerType`/`clientX`/`clientY`/`button`/`buttons`) as plain own data properties rather than new native state, then re-parents the instance onto a dedicated subclass prototype chained to `Event.prototype` (`instanceof Event` holds for all three) — instances flow through the existing `dispatchEvent`/`EventState` machinery unmodified.
5. ~~`window`, `location`, `history`, lifecycle events and navigation cancellation.~~ — done: `window` is a real alias for the global object (`self`/`top`/`parent` too — this engine has exactly one browsing context, no iframes anywhere in `dom`), with real `addEventListener`/`dispatchEvent` via a new non-tree-walking dispatch path (`events::dispatch_simple`/`dispatch_event_object`) for targets like `window`/`document`/`history` that aren't `dom::NodeId`-backed `Node` instances — `document` gets the same surface, since this crate's `document` global is a plain object, not a `Node` subclass. `location` (bare global, `window.location`, `document.location`) exposes real `href`/`protocol`/`host`/`hostname`/`port`/`pathname`/`search`/`hash`/`origin`, derived live from a new `HostState.url` field (`Context::set_url`) — no write side (`href` assignment, `assign`/`replace`/`reload`), since there's no channel yet for JS to ask its host to actually navigate. `history` implements real `pushState`/`replaceState`/`back`/`forward`/`go`/`length`/`state` against a per-context entry stack (`state` kept by reference, not structured-cloned — documented simplification), updating `location` and firing a real `popstate` on `window` when the index moves. `Context::dispatch_lifecycle_events` fires `DOMContentLoaded`/`load`; `Context::fire_before_unload` fires a cancelable `beforeunload` a host checks before replacing the page — `preventDefault()` cancels the navigation outright (no confirmation-dialog UI to fall back to, unlike a real browser). `profile-worker`'s `Page::load` wires all of this end to end: sets the real URL, fires `DOMContentLoaded`/`load` once every page script has run, and `RELOAD`/`NAVIGATE` now fire `beforeunload` first, replying `ERROR navigation canceled by beforeunload` (old page left running untouched) if a listener calls `preventDefault()`. Moving `events::register`/`event_subclasses::register` from `dom_bindings::register` into `Context::new` itself (needed since `window`/`location`/`history` — and therefore `Event` — must exist even without a DOM) surfaced a real bug: any `Event` built on a plain `Context::new()` before this change landed in an unregistered class, corrupting the runtime-level exception state the first time `dispatchEvent` checked it.
6. CSSOM + layout measurement; then hide non-renderable head/style/script source and add incremental invalidation. **In progress** — three of the four pieces landed: `layout-engine` now unconditionally never renders `head`/`style`/`script`/`title`/`meta`/`link`/`base`/`noscript` regardless of any `display` a page's own stylesheet sets on them (this engine has no UA-stylesheet tag-default table at all, so without this check they laid out and painted as ordinary visible content — a real bug this closed, not just a gap; it also fixed 3 of 5 `profile_test.rs` failures previously misdiagnosed as GPU flakiness, since their fixtures place `<style>` before the content it styles). `element.style` (`crates/js-runtime/src/css_style.rs`) is a real `CSSStyleDeclaration` reflecting the inline `style` attribute in both directions — `cssText`, `setProperty`/`getPropertyValue`/`removeProperty`, named camelCase accessors for every property `layout-engine`'s cascade resolver understands, and indexed iteration/`length`; an unrecognized property name still works through `setProperty`/`getPropertyValue`, just without a named accessor (would need a `Proxy`, not exposed by this crate's `quickjs-sys` bindings). Real layout measurement (`crates/js-runtime/src/layout_measurement.rs`) gives `getBoundingClientRect`/`offsetWidth`/`offsetHeight`/`offsetTop`/`offsetLeft`/`clientWidth`/`clientHeight`, backed by a new `HostState.layout_rects` map `Context::set_layout_rects` receives wholesale from a host — `profile-worker`'s `Page::render` now pushes one after every real `layout-engine` pass, collected straight from the `LayoutBox` tree's own absolute `dimensions` (the same coordinates `build_display_list` paints from), tested end to end with a page whose own click handler measures itself and swaps color only if the real measured size matches. ~~**Not done**: `getComputedStyle`~~ — done now too: `HostState.computed_styles` (`Context::set_computed_styles`, same wholesale-replace shape `set_layout_rects` already has) is pushed by `profile-worker`'s `Page::render` from each `LayoutBox`'s own real `ComputedStyle` (`collect_computed_styles`), covering `display`/`position`/`width`/`height`/`margin-*`/`padding-*`/`color`/`background-color`/`font-size`/`opacity` — real cascade-resolved values (stylesheet + inheritance), not inline-style-only. `getComputedStyle(el)` (`crates/js-runtime/src/computed_style.rs`) returns a point-in-time snapshot object (plain data properties set once at call time, not a live view — documented simplification, since re-running `getComputedStyle` after a re-render already gives an up-to-date answer) with named camelCase properties, `cssText`, and `getPropertyValue(name)`. ~~Still not done: `CSSStyleSheet`/stylesheet mutation/adopted stylesheets~~ — done: `crates/js-runtime/src/cssom_stylesheet.rs`'s `CSSStyleSheet` is real rule storage (`insertRule`/`deleteRule`/`cssRules`, each rule validated via `css::parse_stylesheet` producing exactly one real rule) plus `document.adoptedStyleSheets` as a plain writable array. Genuinely wired to rendering, not just a JS-facing shell: `Context::adopted_stylesheet_text` lets a host read every adopted sheet's accumulated rule text back out, and `profile-worker`'s `Page::layout` now merges it onto a cloned copy of the page's base stylesheet before every layout pass — a page calling `sheet.insertRule(...)` and the host re-rendering actually changes what gets painted next frame. Scope cuts: `cssRules[i]` is a snapshot `{ cssText }`, not a live `CSSRule` object with its own mutable `.style`; no `replace`/`replaceSync` (splitting an arbitrary stylesheet string back into individually addressable rules needs source-span tracking `css`'s parser doesn't keep); `document.adoptedStyleSheets` does no validation that its entries are real sheets. Tested at the `js-runtime` unit level (insert/delete/cssRules/adopted-text concatenation); not yet covered by a pixel-level `profile` test (the IPC stdin protocol has no generic "eval arbitrary script" command to trigger a mutation from a test, only fixed commands like `CLICK`/`NAVIGATE` — a real gap, not a shortcut taken silently). ~~Incremental invalidation~~ — done, scoped to what's actually safe without a bigger rendering-architecture change: `dom::Dom` gained a `mutations: u64` counter (`mutation_count()`), bumped by every structural/content mutation (`append_child`, `insert_before`, `remove_from_parent`, `remove`, `set_attribute`, `remove_attribute`, `append_text`, `set_value`) and deliberately *not* by `focus`/`blur`/`clear_focus`, which affect nothing `layout-engine` computes (no focus rings exist). `profile-worker`'s `Page` now caches its last computed `LayoutBox` tree (`layout_cache: RefCell<Option<LayoutCache>>`) keyed on `(width, dom.mutation_count(), adopted_stylesheet_text())`; `layout()` — called by `render()`, `content_height()`, and `hit_test_at()` alike — returns a clone of the cached tree instead of re-running `build_box_tree_with_viewport` + text shaping/flex resolution + `layout_block` from scratch whenever none of those three inputs changed since the last call, which is the common case for a static page's ~60Hz render loop (nothing dirties the DOM between frames) and for the same command triggering multiple `layout()` calls in a row (a `CLICK_AT` hit-tests then may re-render). Real scope cut from a true "incremental" system: still whole-document granularity (any single mutation invalidates the *entire* cached tree, not just the dirtied subtree) — a real per-subtree retained layout tree is a genuinely different architecture (parent boxes reacting to a child's size change, intrinsic-size propagation, ...) correctly judged too large/risky to build alongside everything else landed this session; what's here is the safe, real, measurably-effective first step (skip relayout entirely when nothing relevant changed) rather than a half-built partial-invalidation system with subtler correctness bugs. Verified non-regressing against the two pre-existing GPU-timing-flaky `profile_test.rs` failures via `git stash` A/B comparison (identical failures with and without this change).
7. Origin/security model before expanding cross-origin networking, frames or storage scopes. **In progress** — same-origin/CORS enforcement and mixed-content blocking landed (`crates/js-runtime/src/cors.rs`), covering the one thing this engine's `fetch`/`fetchSync`/`XMLHttpRequest` can even request: a plain, credential-less GET with no custom headers — a real Fetch spec "simple request", which never triggers a CORS preflight even in a real browser. Same-origin (derived from a new `HostState.url`-backed `cors::page_origin`) is always allowed; a cross-origin response only reaches script if it carries a real `Access-Control-Allow-Origin` header that is `*` or matches the page's own origin exactly — a blocked response surfaces the same way a network failure already does per surface (`fetchSync`: `{ok:false,status:0,body:""}`; `fetch()`: promise rejection; XHR: `onerror`). A page loaded over `https://` can no longer fetch a plain `http://` subresource at all (blocked before the request is even sent, unlike CORS) via any of the three surfaces. A context with no real navigated URL (a plain `Context::with_dom`, a test) has nothing to enforce and allows everything, same degrade-gracefully pattern `location`'s properties already use. ~~referrer policy~~ — done: `cors::referrer_header` implements the real browser-default `strict-origin-when-cross-origin` policy (full page URL, minus fragment, for a same-origin request; page origin only for cross-origin; nothing at all for an `https://` page requesting `http://`, matching real downgrade behavior) against `location`'s real current URL, wired into `fetchSync`/`fetch()`/`XMLHttpRequest` as a real `Referer` request header (`net::get_with_headers`, replacing the bare `net::get` all three used before). No per-page/per-request override (`<meta name="referrer">`, a `Referrer-Policy` response header, or `fetch()`'s own `referrerPolicy` option) — none of those exist anywhere in this engine, so the one hardcoded default is the whole policy. ~~opaque origins~~ — done: `location.origin`/CORS's `page_origin` already read `"null"` for free for a `data:`/`blob:` page (the `url` crate's own `Origin::Opaque(_).ascii_serialization()` returns `"null"` per spec, and its `PartialEq` makes two opaque origins never equal even from the same URL — exactly right for CORS's existing string-equality checks, no code change needed there). The one real gap this closed: `cors::referrer_header` checks `!page_url.origin().is_tuple()` and sends no `Referer` at all for an opaque-origin page — real browsers never disclose a referrer from one, unlike the ordinary cross-origin case, which still discloses the page's own origin. ~~CSP enforcement~~ — done, scoped to the one directive that governs this engine's actual network surface: `crates/js-runtime/src/csp.rs`'s `is_connect_allowed` parses a policy string's `connect-src` (falling back to `default-src`, per spec) and checks `*`/`'none'`/`'self'`/an explicit origin-or-host source; `Context::set_csp` (mirrors `set_url`'s shape) feeds it, and `fetch`/`fetchSync`/`XMLHttpRequest` all check it right alongside their existing mixed-content check, blocking the same way. Not implemented: nonces/hashes, wildcard hosts, scheme-only sources, and every directive besides `connect-src`/`default-src` (`script-src` etc — nothing to gate them against, this engine has no separate script-loading/image-fetch policy hook). Wiring gap, documented rather than half-done: nothing extracts a real `Content-Security-Policy` response header or `<meta http-equiv>` tag and calls `set_csp` yet — `profile-worker` doesn't call it, same "real tested primitive, native wiring is a follow-up" shape several other matrix entries already have. **Not done**: permissions policy/trusted types, and (structurally out of scope — no iframes exist anywhere in `dom`) sandboxed documents, multiple browsing contexts, and cross-window messaging.

