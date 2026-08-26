# Nimble — Full Engineering Spec

> Merged from `JS_ENGINE_CAPABILITY_MATRIX.md` (per-feature done/needed capability tracking for the JS engine) and `opt.md` (architecture principles, shared-infrastructure rules, and implementation priority roadmap). Keep both halves in sync as work lands: check off matrix items in Part A, and treat Part B's rules as constraints on *how* new work in Part A should be built.

---

# Part A — JS Engine Capability Matrix

Last reviewed: 2026-08-25. This is an implementation roadmap for Nimble's embedded web engine, not a claim of full browser compatibility. Percentages are weighted estimates of useful, interoperable behaviour in each area; a checked item is implemented and covered by code or integration tests.

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
- [x] `console` (`log`/`info`/`warn`/`error`/`debug`) with leveled message storage hosts can drain, and structured uncaught-error reporting (real exception text + a `window` `error` event).
- [x] Interrupt/time budget for untrusted script execution (`Context::set_time_budget`: quickjs's interrupt handler aborts a budget-exceeding `eval` with an "interrupted" InternalError; deadline re-stamped per call) and memory quotas per document (`Context::set_memory_limit`, quickjs's runtime-wide allocation cap — one runtime per context here, so per-document is exactly what it gives).

**Needed**

- [ ] ES-module loader/resolver, import maps, dynamic `import()` and module cache.
- [ ] Script loading modes: parser-blocking, `defer`, `async`, module scripts and CSP-aware loading.
- [ ] Source maps, debugger protocol and remaining structured error reporting (console + uncaught-error reporting are done — see section 1's Done list; `EvalError` now carries the real stringified exception instead of the old "script raised an exception" placeholder).
- [ ] Worker runtime: `Worker`, `SharedWorker`, `MessageChannel`, structured clone and transferable objects.

## 2. DOM classes, properties and functions

### Implemented surface (38%)

| Class/interface | Implemented properties | Implemented functions |
| --- | --- | --- |
| `Document` | `body`, `documentElement`, `activeElement`, `cookie` | `getElementById`, `createElement`, `createTextNode`, `createDocumentFragment`, `createComment`, `importNode`, `adoptNode`, `querySelector`, `querySelectorAll`, `getElementsByTagName`, `getElementsByClassName` |
| Generic `Node` / element wrapper | `textContent`, `id`, `className`, `parentNode`, `firstChild`, `lastChild`, `previousSibling`, `nextSibling`, `children`, `nodeType`, `nodeName`, `value` | `appendChild`, `insertBefore`, `removeChild`, `remove`, `getAttribute`, `setAttribute`, `removeAttribute`, `contains`, `compareDocumentPosition`, `normalize`, `cloneNode`, `replaceChild`, `querySelector`, `querySelectorAll`, `matches`, `closest`, `focus`, `blur`, `prepend`, `append`, `before`, `after`, `replaceWith`, `insertAdjacentHTML` |
| `EventTarget` | — | `addEventListener`, `removeEventListener`, `dispatchEvent` |
| `Event` | `type`, `target`, `currentTarget`, `bubbles`, `cancelable`, `defaultPrevented` | constructor `new Event(type, options)`, `preventDefault`, `stopPropagation` |
| Form-like nodes | `value`, focus state | native click, input/change/focus/blur dispatch pathways |

### Required DOM roadmap

- [x] Interface hierarchy and correct prototypes: `Node`, `Element`, `HTMLElement`, `HTMLInputElement`, `HTMLButtonElement`, `HTMLAnchorElement`, `HTMLImageElement`, `HTMLCanvasElement`, etc. — done (2026-08-25, `crates/js-runtime/src/dom_bindings.rs`): 8 QuickJS classes registered with shared `JSClassDef` (same `node_finalizer` and `NodeId` opaque). Prototype chain: `HTMLElement.prototype → Element.prototype → Node.prototype → Object.prototype`. Concrete subclasses (`HTMLInputElement`, `HTMLButtonElement`, `HTMLAnchorElement`, `HTMLImageElement`, `HTMLCanvasElement`) chain to `HTMLElement.prototype`. `node_class_id_for(ctx, dom, id)` selects the correct class_id by `NodeData` variant and tag name. `node_opaque` tries all 8 class IDs. Global constructors (`Node`, `Element`, `HTMLElement`, `HTML*`) exposed via `expose_constructor`. All 16 `node_object` call sites updated. 8 `instanceof` tests.
- [ ] `Document`: ~~`head`, `readyState`, `title`, `getElementsBy*`, `createComment`~~ — done (2026-08-25): `document.head` mirrors the existing `body`/`documentElement` getters (first `<head>` in document order); `document.title` reflects the first `<title>` element's text both ways — the setter updates it in place if one exists, otherwise creates one and appends it to `<head>`, else `<html>`, else the document root (narrower than the real spec's SVG-aware fallback chain, since this engine only ever builds HTML documents); `readyState` is a hardcoded `"complete"` (documented placeholder, same convention `page_visibility`'s always-`"visible"` state already uses — no loading/interactive tracking exists); `getElementsByTagName`/`getElementsByClassName` exist on both `document` and `Node.prototype`, matched directly (tag string equality / real class-token-set membership) rather than routed through the CSS selector engine, so a tag or class value containing a selector metacharacter can't be reinterpreted as different selector syntax — non-live snapshot arrays, same simplification `querySelectorAll`/`childNodes`/`attributes` already make; `document.createComment(data)` wraps the pre-existing `dom::Dom::create_comment`. Still `[ ]`: ~~`URL`, `baseURI`, `forms`, `images`, `links`, `scripts`~~ — done (2026-08-25, `crates/js-runtime/src/dom_bindings.rs`): `document.URL`/`baseURI` return the full serialization of the page's URL (empty string when unset; no `<base>` element support, so `baseURI` equals `URL`); `forms`/`images`/`scripts` return an HTMLCollection (snapshot array with `item()`/`namedItem()`, see Collections below) of all `<form>`/`<img>`/`<script>` descendants in document order; `links` is the same collection restricted to `<a>`/`<area>` elements that carry an `href` attribute. ~~`createDocumentFragment`~~ — done (2026-08-25, `crates/dom/src/lib.rs` + `crates/js-runtime/src/dom_bindings.rs`): `document.createDocumentFragment()` creates a `DocumentFragment` node (nodeType 11) that can hold children; ~~`importNode`~~ — done (2026-08-25, `crates/js-runtime/src/dom_bindings.rs`): `document.importNode(node, deep?)` clones a node into this document (single browsing context, so equivalent to `cloneNode(deep)`, `deep` defaults to `true`); ~~`adoptNode`~~ — done (2026-08-25, same file): `document.adoptNode(node)` returns the node as-is (single document, no cross-document transfer needed).
- [ ] `Node`: ~~`ownerDocument`, `isConnected`, `contains`, `cloneNode`, `replaceChild`~~ — done (2026-08-25): `dom::Dom::contains`/`is_connected`/`clone_node`/`replace_child` (`crates/dom/src/lib.rs`) plus their `Node.prototype` bindings (`crates/js-runtime/src/dom_bindings.rs`). `contains`/`isConnected` walk the parent chain (no extra traversal state needed). `cloneNode(deep)` reuses the same tag/attribute-copy shape `Dom::adopt` already established for cross-arena cloning, just within one arena, and returns a real new `Node` object via the existing identity cache (`node_object`) — a fresh id, not the source's own identity. `replaceChild(newChild, oldChild)` rejects a `newChild` that's `this`'s own ancestor (same `HierarchyRequestError`-style check `appendChild`/`insertBefore` already make) and a stale/non-member `oldChild`; it frees `oldChild`'s whole subtree exactly like `removeChild`/`remove` already do — same documented simplification (a real DOM keeps a removed node alive and reattachable; this one destroys it). ~~`replaceWith`, `before`, `after`, `prepend`, `append`~~ — done (2026-08-26, see "Next implementation order" item 9 below): all five variadic, accepting a mix of `Node`/string arguments. ~~`normalize`~~, ~~`compareDocumentPosition`~~ — done (2026-08-25, `crates/dom/src/lib.rs` + `crates/js-runtime/src/dom_bindings.rs`). Still `[ ]`: `childNodes` remains a snapshot NodeList, not a live one that reflects later mutations without re-reading the getter — but it now carries the real `NodeList` API surface (done 2026-08-25: returns an array with `item(index)`, `null` out of range, includes text/comment children).
- [ ] `Element`: ~~`classList`/`DOMTokenList`, `dataset`, `attributes`/`NamedNodeMap`, `innerHTML`, `outerHTML`, `getBoundingClientRect`, `client*`, `offset*`~~ — done in earlier passes (see "Next implementation order" items 1/2/3/6 below); ~~`tagName`, `localName`, `namespaceURI`~~ — done (2026-08-25): exposed generically on `Node.prototype` like every other Element-only property this engine's single `Node` class already carries (`undefined`/`null` off an `Element`), `tagName` uppercased, `localName` in the tag's stored case, `namespaceURI` the fixed HTML-namespace string (this engine never parses/creates SVG/MathML foreign content). ~~`insertAdjacentHTML`~~ — done (2026-08-26, see "Next implementation order" item 10 below). Still `[ ]`: ~~`scroll*`~~ — done (2026-08-25, `crates/js-runtime/src/dom_bindings.rs`) as documented stubs on `Element.prototype`: `scrollTop`/`scrollLeft` getters always return 0 and setters no-op; `scroll()`/`scrollTo()`/`scrollBy()`/`scrollIntoView()` are callable no-ops. This engine has no real viewport (the scroll protocol only exists host-side, per `mockup/rendering-engine-gaps.md` section 7) — the stubs exist so real-world page code that touches them doesn't throw.
- [x] Text/comment/document-fragment nodes and correct `nodeValue` semantics — done (2026-08-25, `crates/dom/src/lib.rs` + `crates/js-runtime/src/dom_bindings.rs`): per-spec `Node.prototype.nodeValue` getter/setter — text data for Text/Comment nodes, `null` for Element/Document/DocumentFragment, setter updates Text/Comment data in place and no-ops elsewhere. The pre-existing generic `value` property (form-element values, fires `"input"`) is unchanged and independent. DocumentFragment nodes themselves existed since `createDocumentFragment`.
- [ ] Collections: live `HTMLCollection`, static `NodeList`, iterable DOM collections. Partially done (2026-08-25): all collection-returning APIs (`getElementsByTagName`, `getElementsByClassName`, `querySelectorAll`, `childNodes`, `document.forms`/`images`/`links`/`scripts`) now return snapshot arrays carrying the real API surface — `HTMLCollection` adds `item(index)` + `namedItem(name)` (matches by `id` or `name` attribute); `NodeList` adds `item(index)` (`null` out of range). Indexed access and `.length` come free from the array base. Still `[ ]`: liveness — a real live HTMLCollection re-evaluates its query on every access and reflects later DOM mutations without re-calling the getter.
- [ ] `DOMParser`, `XMLSerializer`, templates, shadow DOM, custom elements and mutation observers. Partially done (2026-08-25, `crates/js-runtime/src/dom_bindings.rs`): `new DOMParser().parseFromString(html, type)` parses via the real `html::parse_fragment` pipeline into a fresh detached `DocumentFragment` in the shared Dom — queryable (`fragment.querySelector(...)`) and adoptable into the document (documented deviation: spec returns a separate `Document`; single-shared-Dom architecture makes that impossible today). `new XMLSerializer().serializeToString(node)` returns `dom::Dom::serialize_node`'s HTML serialization of any node. Still `[ ]`: `<template>` elements, shadow DOM, custom elements, MutationObserver.
- [ ] HTML parser integration for scripts/styles: do not render their source; model parser insertion modes and document.write policy.
- [ ] Form controls: checked/selected/disabled/name/type/validity, form submission/reset, labels, selection APIs and `FormData`. Partially done (2026-08-25, `crates/js-runtime/src/dom_bindings.rs`): `checked`/`selected`/`disabled` boolean-attribute reflection and generic `.value` (fires `"input"`) predate this pass; new in this pass — two typed interface classes following the existing subclass pattern: `HTMLFormElement` (`form.elements` = HTMLCollection of contained `input`/`select`/`textarea`/`button` controls in document order with `namedItem` support; no-op `submit()`/`reset()` stubs since there's no network layer) and `HTMLSelectElement` (`options` HTMLCollection of `<option>` descendants, `selectedIndex` getter/setter whose setter deselects the others, `value` getter/setter reading/writing the option's effective value — its `value` attribute or text content). Documented deviation: fresh single-selects read `selectedIndex === -1`/`value === ""` until something is explicitly selected (browsers implicitly select the first option). ~~`validity`/constraint APIs, `labels`~~ — done (2026-08-25, `crates/js-runtime/src/dom_bindings.rs`, all on `Node.prototype` per this engine's single-class convention): `validity` returns a fresh object with all ten per-spec flags — computed: `valueMissing` (`required` + empty effective value), `tooShort`/`tooLong` (`minlength`/`maxlength` vs character count, non-empty values only), `customError`; always-false: `typeMismatch`/`patternMismatch`/`rangeUnderflow`/`rangeOverflow`/`stepMismatch` (no regex/range validation natively, documented cut); `checkValidity()` fires a real `"invalid"` event on failure; `reportValidity()` same boolean without the event; `setCustomValidity(msg)` stores the message as an own property on the wrapper object (tag-checked read — `JS_ToCStringLen2` coerces missing properties' `undefined` into the literal string `"undefined"`); `validationMessage` mirrors it; `willValidate` is true for enabled `input`/`select`/`textarea`; `labels` collects `<label for>` matches document-wide plus wrapping `<label>` ancestors (not re-sorted into full tree order, documented); `htmlFor` reflects the `for` attribute both ways. Still `[ ]`: real submission/reset semantics, selection APIs (`setSelectionRange` etc.). ~~`FormData`~~ — done (2026-08-25, `crates/js-runtime/src/form_data.rs`): real class with ordered entries — `append`/`set` (string or Blob/File values with optional filename), `get`/`getAll`/`has`/`delete`, snapshot-walk `forEach`, `entries()`/`keys()`/`values()`. `new FormData(form)` populates from named controls in document order (checkbox/radio only when checked with `"on"` fallback, disabled and unnamed controls skipped, selects contribute their explicitly-selected option's effective value, textarea via `.value` text fallback; buttons excluded since no submitter concept exists). Appended Blob/File values are byte snapshots; `get()` returns a fresh genuine Blob carrying `name`/`lastModified`. Documented deviations: `entries()`/`keys()`/`values()` return plain arrays not iterator objects (no well-known-symbol binding in `quickjs-sys`), DOM-less contexts degrade to empty instances, fetch body integration deferred (fetch has no request-body support at all yet).

## 3. Events and input

**Done (55%)**

- [x] Stable element identity across lookups.
- [x] Ordered listeners, listener removal, listener count cap and exception isolation.
- [x] Event construction and option flags.
- [x] Target phase and bounded bubbling; cancellation and propagation stop.
- [x] Native profile-worker dispatch for click, keyboard input, focus, blur, input and change.

**Needed**

- [ ] Capturing phase, `once`, `passive`, `signal`, listener objects and full event path/composed semantics.
- [x] Event subclasses: ~~`MouseEvent`~~, ~~`PointerEvent`~~, ~~`KeyboardEvent`~~, ~~`InputEvent`~~, ~~`FocusEvent`~~, ~~`SubmitEvent`~~, ~~`CustomEvent`~~, ~~`WheelEvent`~~, ~~`TouchEvent`~~, ~~`DragEvent`~~ — `CustomEvent`/`KeyboardEvent`/`PointerEvent` done in an earlier pass (see "Next implementation order" item 4); `MouseEvent` (`screenX`/`screenY`/`clientX`/`clientY`/`ctrlKey`/`shiftKey`/`altKey`/`metaKey`/`button`/`buttons`/`relatedTarget`) and `FocusEvent` (`relatedTarget`) done (2026-08-25, `crates/js-runtime/src/event_subclasses.rs`), same pattern as the earlier three: plain own data properties on top of `events::create_event`'s base `Event` object, prototype chained onto `Event.prototype` so `instanceof Event` holds. `relatedTarget` is the first field on any of these subclasses that's a `Node` reference rather than a primitive — read via a new `read_value_option` helper (no coercion, defaults to `null`) and written with a new `set_value_prop` that transfers ownership of the already-duped `JSValue` straight into `JS_SetPropertyStr`, same convention `detail`'s object-valued handling in `custom_event_constructor` already used ad hoc. `InputEvent` (`data`/`inputType`/`isComposing`) done (2026-08-25, same file): `data` reuses `relatedTarget`'s `read_value_option`/`set_value_prop` pair since the real API allows `data` to be `null` (composition/deletion events carry no string), not a primitive-only field like `inputType`/`isComposing`. ~~`WheelEvent`~~ — done (2026-08-25, see "Next implementation order" item 11 for the full writeup): `deltaX`/`deltaY`/`deltaZ`/`deltaMode`/`clientX`/`clientY`. Still `[ ]`: `SubmitEvent`, `TouchEvent`, `DragEvent`.
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

Update — ~~permissions policy~~ is now implemented for the capability surfaces Nimble actually exposes: `Context::set_permissions_policy` retains the host-provided policy and checks `clipboard-read`/`clipboard-write` before `navigator.clipboard` reaches the OS, and `notifications` before `Notification.requestPermission()` changes state or `new Notification(...)` creates an instance. The accepted top-level subset is `feature=()` (deny), `feature=(self)`, and `feature=(*)`; origin delegation remains structurally out of scope because Nimble has no iframes. ~~Trusted Types~~ - done, see item 7 below.

## 8. Quality gates and measurable milestones

1. **DOM usability — 55% overall**: add `classList`, `dataset`, `innerHTML`, `NodeList`/collections, element-specific prototypes and capture-phase events.
2. **Application compatibility — 65% overall**: `window`/location/history, fetch request-response objects, CSSOM, form semantics, layout measurement and default actions.
3. **Browser correctness — 75% overall**: origin/CORS/CSP/lifecycle/iframe model plus script loading modes and incremental rendering.
4. **Platform readiness — 85% overall**: broad Web Platform Tests, accessibility tree, devtools diagnostics, quotas/timeouts and performance profiling.

## Next implementation order

1. ~~`DOMTokenList` via `element.classList`~~ — done: `add`/`remove`/`contains` (already landed) plus `toggle`/`replace`/`value` getter-setter/`length`/real iteration now real too (`crates/js-runtime/src/dom_bindings.rs`, `sync_class_list`). Stable per-`NodeId` object identity kept (`CLASS_LIST_OBJECTS` cache), refreshed from the live `class` attribute on every mutating call and on every `element.classList` getter hit so a direct `setAttribute("class", ...)` bypass is still picked up on next access. Iteration is real `Symbol.iterator`/`forEach`/etc, not a hand-rolled protocol: the classList object's prototype is set to the real `Array.prototype` (`array_prototype`, via `JS_SetPrototype`) and indexed properties (`0..length`) are kept in sync — `Array.prototype`'s own iterator is generic over any array-like `this`, so this is genuinely spec-shaped, not array-typed (`Array.isArray(classList)` is still `false`, matching real `DOMTokenList`). No `quickjs-sys` binding for well-known symbols was needed or added. `item(index)` isn't a separate method — falls out of the same indexed properties.
2. ~~`dataset` and `attributes`~~ — done. `element.dataset` (`sync_dataset`/`node_dataset_get`) reflects every live `data-*` attribute under its camelCase key (`data-user-id` -> `dataset.userId`), stable per-node identity, resynced on every access; read-side only (`el.dataset.foo = "x"` sets a plain JS property, doesn't write back — real per-key reflection needs a `Proxy`, not exposed by this crate's `quickjs-sys` bindings, same documented scope-down `localStorage`'s missing bracket access already has). `element.attributes` (`sync_attributes`/`node_attributes_get`) is a stable, live, iterable `{name, value}` collection — inherits `Array.prototype` (same `array_prototype` trick `classList` uses) for real `Symbol.iterator`/`length`, plus `getNamedItem(name)`; entries are snapshot objects, not live `Attr` nodes (mutating `.value` on an entry doesn't write back — same read-reflects-live, write-doesn't scope as `dataset`). ~~Next: element-specific properties for form controls and anchors.~~ — done: `checked`/`disabled`/`selected` reflect HTML boolean-attribute semantics (presence-only — `boolean_attribute_get`/`_set` in `dom_bindings.rs`, distinct from `attribute_property_get`/`_set`'s string reflection), `href` reuses the plain string reflection anchors already share with `id`/`className`/`name`/`type`.
3. ~~`innerHTML`/`outerHTML` backed by the existing HTML parser, with bounded input and listener/node-cache invalidation.~~ — done: `dom::Dom::serialize_children`/`serialize_node` (the read side, attributes sorted by name for determinism since this crate's storage has no authored order) plus `dom::Dom::adopt` (deep-clones a subtree across two different `Dom` arenas — `NodeId`s are only valid within the arena that created them) and `html::parse_fragment` (parses a string the same way a full document is parsed, returns the resulting `<body>`'s children) give the write side something to build on. `crates/js-runtime/src/dom_bindings.rs`'s `innerHTML`/`outerHTML` getter/setters wire it together — a setter evicts cached JS state (`NODE_OBJECTS`/`classList`/`dataset`/`attributes`, and any listeners riding on those cached objects) for every removed node first, same convention `removeChild`/`remove` already use, and input is length-bounded (64KB) like this file's other untrusted-string setters. No void-element list exists in this engine (`<br></br>` is acceptable output), and `outerHTML`'s write side makes the node stop existing afterward, matching `remove()`'s own contract.
4. ~~Capture phase and event listener options; then `CustomEvent`, keyboard and pointer event classes.~~ — done: `events.rs`'s dispatch is a real three-phase traversal now (capturing root->target, at-target, bubbling target->root) instead of a single bubble-only pass; `addEventListener`/`removeEventListener` accept the `useCapture` boolean or a `{capture, once, passive}` options object, plus `EventListener` objects exposing `handleEvent` — `once` removes the listener right after it fires, `passive` makes `preventDefault()` a no-op for the duration of that listener's call. `signal` (`AbortSignal`) is a documented scope cut, no `AbortController` exists yet. Building this surfaced a real latent bug: `Event`'s constructor never exposed a JS-visible `Event.prototype` property (only the internal per-context class proto via `JS_SetClassProto`), so anything reading `Event.prototype` — `instanceof` checks, subclassing — saw `undefined`; fixed by also setting it as a real own property on the `Event` constructor, holding the same object `JS_SetClassProto` registers. `events::create_event` is now exposed as the seam `event_subclasses.rs`'s three new global constructors (`CustomEvent`, `KeyboardEvent`, `PointerEvent`) build on: each sets its subclass-specific fields (`detail`; `key`/`code`/`keyCode`/`ctrlKey`/etc; `pointerId`/`pointerType`/`clientX`/`clientY`/`button`/`buttons`) as plain own data properties rather than new native state, then re-parents the instance onto a dedicated subclass prototype chained to `Event.prototype` (`instanceof Event` holds for all three) — instances flow through the existing `dispatchEvent`/`EventState` machinery unmodified. ~~`MouseEvent`, `FocusEvent`~~ — done (2026-08-25): same file, same pattern, two more global constructors registered alongside the first three. `MouseEvent` adds `screenX`/`screenY`/`clientX`/`clientY`/`ctrlKey`/`shiftKey`/`altKey`/`metaKey`/`button`/`buttons`/`relatedTarget`; `FocusEvent` adds just `relatedTarget`. `relatedTarget` is the first field here that's a `Node` reference instead of a primitive — a new `read_value_option`/`set_value_prop` pair handles the no-coercion read (defaults to `null`) and the ownership-transferring write, generalizing the ad hoc object handling `custom_event_constructor`'s `detail` field already did inline. Tested (`crates/js-runtime/tests/event_subclasses_test.rs`): field/default round-trips for both constructors, and a real `Node` instance surviving the `relatedTarget` round-trip (`e.relatedTarget === other`). Still open on this roadmap line: `InputEvent`, `SubmitEvent`, `WheelEvent`, `TouchEvent`, `DragEvent` (section 3's own roadmap bullet).
5. ~~`window`, `location`, `history`, lifecycle events and navigation cancellation.~~ — done: `window` is a real alias for the global object (`self`/`top`/`parent` too — this engine has exactly one browsing context, no iframes anywhere in `dom`), with real `addEventListener`/`dispatchEvent` via a new non-tree-walking dispatch path (`events::dispatch_simple`/`dispatch_event_object`) for targets like `window`/`document`/`history` that aren't `dom::NodeId`-backed `Node` instances — `document` gets the same surface, since this crate's `document` global is a plain object, not a `Node` subclass. `location` (bare global, `window.location`, `document.location`) exposes real `href`/`protocol`/`host`/`hostname`/`port`/`pathname`/`search`/`hash`/`origin`, derived live from a new `HostState.url` field (`Context::set_url`) — no write side (`href` assignment, `assign`/`replace`/`reload`), since there's no channel yet for JS to ask its host to actually navigate. `history` implements real `pushState`/`replaceState`/`back`/`forward`/`go`/`length`/`state` against a per-context entry stack (`state` kept by reference, not structured-cloned — documented simplification), updating `location` and firing a real `popstate` on `window` when the index moves. `Context::dispatch_lifecycle_events` fires `DOMContentLoaded`/`load`; `Context::fire_before_unload` fires a cancelable `beforeunload` a host checks before replacing the page — `preventDefault()` cancels the navigation outright (no confirmation-dialog UI to fall back to, unlike a real browser). `profile-worker`'s `Page::load` wires all of this end to end: sets the real URL, fires `DOMContentLoaded`/`load` once every page script has run, and `RELOAD`/`NAVIGATE` now fire `beforeunload` first, replying `ERROR navigation canceled by beforeunload` (old page left running untouched) if a listener calls `preventDefault()`. Moving `events::register`/`event_subclasses::register` from `dom_bindings::register` into `Context::new` itself (needed since `window`/`location`/`history` — and therefore `Event` — must exist even without a DOM) surfaced a real bug: any `Event` built on a plain `Context::new()` before this change landed in an unregistered class, corrupting the runtime-level exception state the first time `dispatchEvent` checked it.
6. ~~CSSOM + layout measurement; then hide non-renderable head/style/script source and add incremental invalidation.~~ — done. Three of the four pieces landed first: `layout-engine` now unconditionally never renders `head`/`style`/`script`/`title`/`meta`/`link`/`base`/`noscript` regardless of any `display` a page's own stylesheet sets on them (this engine has no UA-stylesheet tag-default table at all, so without this check they laid out and painted as ordinary visible content — a real bug this closed, not just a gap; it also fixed 3 of 5 `profile_test.rs` failures previously misdiagnosed as GPU flakiness, since their fixtures place `<style>` before the content it styles). `element.style` (`crates/js-runtime/src/css_style.rs`) is a real `CSSStyleDeclaration` reflecting the inline `style` attribute in both directions — `cssText`, `setProperty`/`getPropertyValue`/`removeProperty`, named camelCase accessors for every property `layout-engine`'s cascade resolver understands, and indexed iteration/`length`; an unrecognized property name still works through `setProperty`/`getPropertyValue`, just without a named accessor (would need a `Proxy`, not exposed by this crate's `quickjs-sys` bindings). Real layout measurement (`crates/js-runtime/src/layout_measurement.rs`) gives `getBoundingClientRect`/`offsetWidth`/`offsetHeight`/`offsetTop`/`offsetLeft`/`clientWidth`/`clientHeight`, backed by a new `HostState.layout_rects` map `Context::set_layout_rects` receives wholesale from a host — `profile-worker`'s `Page::render` now pushes one after every real `layout-engine` pass, collected straight from the `LayoutBox` tree's own absolute `dimensions` (the same coordinates `build_display_list` paints from), tested end to end with a page whose own click handler measures itself and swaps color only if the real measured size matches. ~~**Not done**: `getComputedStyle`~~ — done now too: `HostState.computed_styles` (`Context::set_computed_styles`, same wholesale-replace shape `set_layout_rects` already has) is pushed by `profile-worker`'s `Page::render` from each `LayoutBox`'s own real `ComputedStyle` (`collect_computed_styles`), covering `display`/`position`/`width`/`height`/`margin-*`/`padding-*`/`color`/`background-color`/`font-size`/`opacity` — real cascade-resolved values (stylesheet + inheritance), not inline-style-only. `getComputedStyle(el)` (`crates/js-runtime/src/computed_style.rs`) returns a point-in-time snapshot object (plain data properties set once at call time, not a live view — documented simplification, since re-running `getComputedStyle` after a re-render already gives an up-to-date answer) with named camelCase properties, `cssText`, and `getPropertyValue(name)`. ~~Still not done: `CSSStyleSheet`/stylesheet mutation/adopted stylesheets~~ — done: `crates/js-runtime/src/cssom_stylesheet.rs`'s `CSSStyleSheet` is real rule storage (`insertRule`/`deleteRule`/`cssRules`, each rule validated via `css::parse_stylesheet` producing exactly one real rule) plus `document.adoptedStyleSheets` as a plain writable array. Genuinely wired to rendering, not just a JS-facing shell: `Context::adopted_stylesheet_text` lets a host read every adopted sheet's accumulated rule text back out, and `profile-worker`'s `Page::layout` now merges it onto a cloned copy of the page's base stylesheet before every layout pass — a page calling `sheet.insertRule(...)` and the host re-rendering actually changes what gets painted next frame. Scope cuts: `cssRules[i]` is a snapshot `{ cssText }`, not a live `CSSRule` object with its own mutable `.style`; no `replace`/`replaceSync` (splitting an arbitrary stylesheet string back into individually addressable rules needs source-span tracking `css`'s parser doesn't keep); `document.adoptedStyleSheets` does no validation that its entries are real sheets. Tested at the `js-runtime` unit level (insert/delete/cssRules/adopted-text concatenation); ~~not yet covered by a pixel-level `profile` test~~ — covered now: the IPC stdin protocol's missing generic "eval arbitrary script" command landed as `EVAL <script>` (`crates/profile/src/bin/profile_worker.rs`, host-side `profile::Profile::evaluate`) — one line of JS evaluated in the current page's own context, frame re-rendered and republished *before* the reply (`EVALUATED <stringified result>` / `ERROR script raised an exception`), so a test's mutation is already visible in real pixels by the time the call returns; scripts are newline-free by protocol necessity and rejected for NUL bytes/64KB+ length host-side (NUL would panic `Context::eval`'s `CString::new` conversion — a malformed stdin command must not crash the worker), and evaluation is deliberately not CSP-gated (host-driven devtools-console-style evaluation, not page-loaded script). The deferred test exists now too: `an_adopted_stylesheets_insert_rule_actually_changes_painted_pixels` navigates a red-marker page, EVALs `new CSSStyleSheet()` + `insertRule('#marker { … #00ff00 }')` + `document.adoptedStyleSheets = [sheet]`, and asserts the published frame flipped red→green — which can only happen if the mutation genuinely flowed eval → `adopted_stylesheet_text` → cascade merge → relayout (cache invalidated by its adopted-text key) → paint. Two more tests cover the command itself: completion-value round-trip (`"2 + 3"` → `"5"`, plus a live-DOM read proving it runs in the page's context) and throwing-script behavior (`Err` reply, worker stays responsive via `PING`, next `EVAL` still runs). ~~Incremental invalidation~~ — done, scoped to what's actually safe without a bigger rendering-architecture change: `dom::Dom` gained a `mutations: u64` counter (`mutation_count()`), bumped by every structural/content mutation (`append_child`, `insert_before`, `remove_from_parent`, `remove`, `set_attribute`, `remove_attribute`, `append_text`, `set_value`) and deliberately *not* by `focus`/`blur`/`clear_focus`, which affect nothing `layout-engine` computes (no focus rings exist). `profile-worker`'s `Page` now caches its last computed `LayoutBox` tree (`layout_cache: RefCell<Option<LayoutCache>>`) keyed on `(width, dom.mutation_count(), adopted_stylesheet_text())`; `layout()` — called by `render()`, `content_height()`, and `hit_test_at()` alike — returns a clone of the cached tree instead of re-running `build_box_tree_with_viewport` + text shaping/flex resolution + `layout_block` from scratch whenever none of those three inputs changed since the last call, which is the common case for a static page's ~60Hz render loop (nothing dirties the DOM between frames) and for the same command triggering multiple `layout()` calls in a row (a `CLICK_AT` hit-tests then may re-render). Real scope cut from a true "incremental" system: still whole-document granularity (any single mutation invalidates the *entire* cached tree, not just the dirtied subtree) — a real per-subtree retained layout tree is a genuinely different architecture (parent boxes reacting to a child's size change, intrinsic-size propagation, ...) correctly judged too large/risky to build alongside everything else landed this session; what's here is the safe, real, measurably-effective first step (skip relayout entirely when nothing relevant changed) rather than a half-built partial-invalidation system with subtler correctness bugs. Verified non-regressing against the two pre-existing GPU-timing-flaky `profile_test.rs` failures via `git stash` A/B comparison (identical failures with and without this change).
7. Origin/security model before expanding cross-origin networking, frames or storage scopes. **Done** — same-origin/CORS enforcement and mixed-content blocking landed (`crates/js-runtime/src/cors.rs`), covering the one thing this engine's `fetch`/`fetchSync`/`XMLHttpRequest` can even request: a plain, credential-less GET with no custom headers — a real Fetch spec "simple request", which never triggers a CORS preflight even in a real browser. Same-origin (derived from a new `HostState.url`-backed `cors::page_origin`) is always allowed; a cross-origin response only reaches script if it carries a real `Access-Control-Allow-Origin` header that is `*` or matches the page's own origin exactly — a blocked response surfaces the same way a network failure already does per surface (`fetchSync`: `{ok:false,status:0,body:""}`; `fetch()`: promise rejection; XHR: `onerror`). A page loaded over `https://` can no longer fetch a plain `http://` subresource at all (blocked before the request is even sent, unlike CORS) via any of the three surfaces. A context with no real navigated URL (a plain `Context::with_dom`, a test) has nothing to enforce and allows everything, same degrade-gracefully pattern `location`'s properties already use. ~~referrer policy~~ — done: `cors::referrer_header` implements the real browser-default `strict-origin-when-cross-origin` policy (full page URL, minus fragment, for a same-origin request; page origin only for cross-origin; nothing at all for an `https://` page requesting `http://`, matching real downgrade behavior) against `location`'s real current URL, wired into `fetchSync`/`fetch()`/`XMLHttpRequest` as a real `Referer` request header (`net::get_with_headers`, replacing the bare `net::get` all three used before). No per-page/per-request override (`<meta name="referrer">`, a `Referrer-Policy` response header, or `fetch()`'s own `referrerPolicy` option) — none of those exist anywhere in this engine, so the one hardcoded default is the whole policy. ~~opaque origins~~ — done: `location.origin`/CORS's `page_origin` already read `"null"` for free for a `data:`/`blob:` page (the `url` crate's own `Origin::Opaque(_).ascii_serialization()` returns `"null"` per spec, and its `PartialEq` makes two opaque origins never equal even from the same URL — exactly right for CORS's existing string-equality checks, no code change needed there). The one real gap this closed: `cors::referrer_header` checks `!page_url.origin().is_tuple()` and sends no `Referer` at all for an opaque-origin page — real browsers never disclose a referrer from one, unlike the ordinary cross-origin case, which still discloses the page's own origin. ~~CSP enforcement~~ — done, scoped to the one directive that governs this engine's actual network surface: `crates/js-runtime/src/csp.rs`'s `is_connect_allowed` parses a policy string's `connect-src` (falling back to `default-src`, per spec) and checks `*`/`'none'`/`'self'`/an explicit origin-or-host source; `Context::set_csp` (mirrors `set_url`'s shape) feeds it, and `fetch`/`fetchSync`/`XMLHttpRequest` all check it right alongside their existing mixed-content check, blocking the same way. Not implemented: nonces/hashes, wildcard hosts, scheme-only sources, and every directive besides `connect-src`/`default-src` (`script-src` etc — nothing to gate them against, this engine has no separate script-loading/image-fetch policy hook). ~~Wiring gap: nothing extracts a real `Content-Security-Policy` response header or `<meta http-equiv>` tag and calls `set_csp`.~~ — closed (2026-08-25): `profile-worker`'s page load now delivers real policies end to end (`resolve_document` keeps the document response's own `Content-Security-Policy` headers — repeated headers stay separate entries; `collect_meta_csp_policies` walks the parsed DOM for `<meta http-equiv="Content-Security-Policy" content="...">`, ASCII case-insensitive `http-equiv` match), and `Page::load` hands each to the new `Context::add_csp_policy` *before any page script runs*, matching a real browser's "the policy is fully in effect by the time your first script executes". Policies append rather than join — `HostState.csp` became a `Vec<String>` and `is_request_blocked` requires *every* delivered policy to allow the request, because real CSP policies intersect rather than merge and joining two strings into one would silently loosen enforcement (e.g. `connect-src *` next to `default-src 'none'`); `Context::set_csp` keeps its wholesale-replace contract for hosts that set one policy themselves. Tested end to end in `profile_test.rs`: a server sending the header blocks the page's own same-origin `fetchSync` (visible red marker), a `<meta>` tag does too, and the identical page without either delivery channel paints green (control). **Not done**: ~~trusted types~~ - done: `crates/js-runtime/src/trusted_types.rs` gives every context a real `trustedTypes` global whose `createPolicy(name, handlers)` mints policy objects carrying a user-supplied `createHTML` callback plus a native `createHTML(string)` method that runs it and wraps the result in an opaque `TrustedHTML` instance (a registered QuickJS class, `Box<String>` opaque, finalizer-owned, `toString` returning the data - the value is genuinely typed, so `String(v)` round-trips but a plain string never masquerades as one). The CSP directive `require-trusted-types-for 'script'` (`csp.rs`'s `is_trusted_types_required`, parsed from any delivered policy via the existing `find_directive`) gates this engine's only HTML injection sinks: `innerHTML`/`outerHTML` assignment throws when handed anything that isn't a `TrustedHTML`, with the spec's own message shape ("Failed to set the 'innerHTML' property on 'Element': This document requires 'TrustedHTML' assignment"), while a policy-created value passes whether or not enforcement is on. Enforcement follows real multi-policy semantics - delivered policies intersect, so `is_trusted_types_required` fires if *any* of them carries the directive; `Context::set_csp`'s wholesale-replace contract means a replacement without the directive turns enforcement back off. Wired end to end through the existing delivery pipeline (header + `<meta http-equiv>`, both already feeding `add_csp_policy` before page scripts run): `profile_test.rs` navigates a meta-delivered page whose script tries a direct `innerHTML` injection in a try/catch and paints green on TypeError / blue on success - asserted green under enforcement, blue for the identical control page without the tag, and green again for a page that routes through `createPolicy(...).createHTML(...)` (the correct-usage escape hatch). Scope cuts: only `createHTML` exists on policies (no `TrustedScript`/`TrustedScriptURL` types exist to gate - this engine has no script-URL or eval-from-page sink); the `trusted-types <names>` allowlist directive is parsed nowhere and not enforced; `defaultPolicy` is always `null` (the setCSPDefaultPolicy header doesn't exist anywhere in this engine's plumbing); no `getAttributeType`/`getPropertyType` reporting; a policy callback must be invoked as a method (`policy.createHTML(x)` - a destructured reference loses the handler by construction, since the native method reads `_createHTML` off `this_val` rather than Rust holding a `JSValue` across calls). Duplicate policy-name tracking rides `HostState.trusted_type_policy_names`, so it degrades to accepting duplicates on a bare `Context::new()` with no host state - same graceful degradation every other HostState-backed feature has. Still structurally out of scope — no iframes exist anywhere in `dom`: sandboxed documents, multiple browsing contexts, and cross-window messaging.
8. DOM tree-relationship and identity completions from section 2's `Node`/`Element`/`Document` roadmap bullets, picking up where classList/dataset/innerHTML left off. `Node`: `contains`/`isConnected`/`cloneNode`/`replaceChild`/`ownerDocument` — done (2026-08-25): `dom::Dom::contains`/`is_connected`/`clone_node`/`replace_child` (`crates/dom/src/lib.rs`) plus `Node.prototype` bindings. `contains`/`isConnected` walk the parent chain; `cloneNode(deep)` reuses `Dom::adopt`'s tag/attribute-copy shape within one arena and returns a fresh identity via the existing `node_object` cache; `replaceChild` rejects a `newChild` that's this node's own ancestor (same check `appendChild`/`insertBefore` already make) and frees the old child's subtree exactly like `removeChild`/`remove` already do. `Element`/`Document`: `tagName`/`localName`/`namespaceURI`, `document.head`/`title`/`readyState`/`createComment`, and `getElementsByTagName`/`getElementsByClassName` (on both `document` and `Node.prototype`) — done (2026-08-25). The tag/class lookups are matched directly (string equality / real class-token-set membership), not routed through the CSS selector engine like `querySelector`/`matches` already are, specifically so a tag or class value containing a selector metacharacter can't be reinterpreted as selector syntax — non-live snapshot arrays, same simplification `querySelectorAll` already makes. `namespaceURI` returns the fixed HTML-namespace string for every `Element` (this engine never parses/creates SVG/MathML foreign content, so that's genuinely correct, not a placeholder). Still open on these same roadmap lines: `childNodes` as a real live `NodeList` (still a plain snapshot array); ~~`normalize`/`compareDocumentPosition`~~ done (2026-08-25); ~~`createDocumentFragment`~~ done (2026-08-25); ~~`importNode`/`adoptNode`~~ done (2026-08-25), and the full `HTMLCollection`/`NodeList`/interface-hierarchy work section 2 still lists as `[ ]`.
9. `ChildNode`/`ParentNode` mixin methods — `prepend`/`append`/`before`/`after`/`replaceWith` — done (2026-08-26): all five are variadic (`Node.prototype`, `crates/js-runtime/src/dom_bindings.rs`), each argument either a real string primitive (checked via `arg.tag == JS_TAG_STRING` *before* any coercion — `read_js_string` itself would happily stringify a `Node` object through its own `toString`, which is not what a `Node` argument means here) turned into a fresh Text node, or an existing `Node` reused by id; bounded by a new `MAX_CHILD_NODE_ARGS` (256) alongside the pre-existing `MAX_TEXT_NODE_LENGTH`, same "cap the untrusted input" convention every other entry point in this file already follows. `prepend`/`append` insert at this node's leading/trailing edge; `before`/`after` insert relative to this node in its parent (a no-op if this node has no parent, matching the real spec's own "if parent is null, then return"); `replaceWith` removes this node and inserts the arguments at the position it occupied, freeing this node's own subtree exactly like `removeChild`/`remove` already do (documented simplification: a real DOM keeps a removed node alive and reattachable, this one destroys it — same deviation `replaceChild` already has). All five reuse the existing `is_ancestor`/`insert_before`/`append_child` primitives directly — no new `dom::Dom` methods were needed, since none of these operations is more than a sequence of the tree-mutation primitives that already existed. Tested (`crates/js-runtime/tests/js_runtime_test.rs`): mixed Node/string arguments landing in argument order for `prepend`/`append`, `before`/`after` relative to a reference node (plus a detached-node no-op), `replaceWith` swapping position and freeing the old node (`document.getElementById` on it afterward returns `null`), and both a hierarchy-cycle argument and a wrong-type (number) argument being rejected with a thrown `TypeError` while leaving the tree untouched.
10. `Element.insertAdjacentHTML(position, html)` — done (2026-08-26): reuses the exact `html::parse_fragment` + `Dom::adopt` pipeline `innerHTML`/`outerHTML` already established, inserting the parsed roots at one of the four real positions (`beforebegin`/`afterbegin`/`beforeend`/`afterend`) via the same `insert_before`/`append_child` primitives `before`/`after`/`prepend`/`append` (item 9 above) already reuse — `afterbegin`/`beforeend` share `prepend`/`append`'s "first child or append" / plain-append logic, `beforebegin`/`afterend` share `before`/`after`'s "insert before this / before the captured next sibling" logic, just against parsed-and-adopted nodes instead of JS-supplied ones. `beforebegin`/`afterend` throw (this file's usual plain `TypeError`, not a distinct `NoModificationAllowedError` type — no DOMException hierarchy exists anywhere in this crate) when this node has no parent, matching the real spec. Goes through the same `trusted_types::sink_html_string` gate `innerHTML`/`outerHTML` already use — this engine's three real HTML injection sinks now share identical Trusted Types enforcement. Tested (`crates/js-runtime/tests/js_runtime_test.rs`): all four positions landing in the right place relative to both the target node and its existing children, an invalid position string rejected, and a parent-less node rejected for `beforebegin` while still accepting `afterbegin` (no parent required).
11. `MouseEvent`/`FocusEvent` (section 3's Event-subclasses roadmap line) — done (2026-08-25, see item 4 above for the full writeup). `InputEvent` — done (2026-08-25, `crates/js-runtime/src/event_subclasses.rs`): `data`/`inputType`/`isComposing` as plain own data properties, same constructor-registration pattern as every other subclass in this file; `data` defaults to `null` (via `read_value_option`, no coercion) rather than an empty string, matching the real API's "no text for this input type" case. Tested (`crates/js-runtime/tests/event_subclasses_test.rs`): field round-trip plus `instanceof Event`, and the no-options-given default (`data === null`, `inputType === ""`, `isComposing === false`). ~~`WheelEvent`~~ — done (2026-08-25, `crates/js-runtime/src/event_subclasses.rs`): `deltaX`/`deltaY`/`deltaZ`/`deltaMode`/`clientX`/`clientY` as plain own data properties, same `register_subclass` pattern as every other subclass in this file; `deltaMode` defaults to `0` (`DOM_DELTA_PIXEL`, the real spec's own default). Tested (`crates/js-runtime/tests/event_subclasses_test.rs`): field round-trip plus `instanceof Event`/`instanceof WheelEvent`, and the no-options-given all-zero default. Closes the JS-constructible-class half only — no native wheel dispatch pathway exists yet to construct one from an actual scroll input (`SubmitEvent`/`TouchEvent`/`DragEvent` have the same gap).

---

# Part B — Architecture and Implementation Optimization Plan

> This document complements the existing Nimble implementation roadmap. Its purpose is to define architectural principles, implementation priorities, performance rules, and shared infrastructure that should guide future development.
>
> The goal is not only to implement more Web APIs, but to ensure that every new feature improves the engine without causing uncontrolled complexity, duplicated logic, excessive allocations, global invalidation, or future rewrites.

---

## 1. Goals

Nimble already contains a significant amount of functionality across:

- JavaScript execution;
- DOM;
- events;
- CSS;
- layout;
- rendering;
- navigation;
- timers;
- networking;
- storage;
- Rust ↔ JavaScript bindings.

The primary risk going forward is no longer simply missing APIs.

The larger risk is architectural fragmentation:

```text
New API
   ↓
New binding-specific implementation
   ↓
New state
   ↓
New cache
   ↓
New invalidation logic
   ↓
More coupling
   ↓
Harder future implementations
````

Future work should prioritize:

1. Shared infrastructure over API-specific implementations.
2. Centralized mutation and invalidation.
3. Lazy recomputation where possible.
4. Typed and allocation-efficient hot paths.
5. Explicit ownership and object lifetime.
6. Measurable performance improvements.
7. Compatibility testing alongside implementation.
8. Infrastructure that can support multiple future Web APIs.

---

## 2. Architecture Before APIs

Before implementing a new Web API, first answer:

> Can this functionality reuse an existing shared internal primitive?

Do not prefer this architecture:

```text
API A
└── Custom state
    └── Custom scheduler
        └── Custom resource loader

API B
└── Custom state
    └── Custom scheduler
        └── Custom resource loader
```

Prefer:

```text
                    ┌─────────────────────┐
                    │ Shared Infrastructure│
                    ├─────────────────────┤
                    │ DOM Core            │
                    │ Mutation Pipeline   │
                    │ Task Scheduler      │
                    │ Resource Loader     │
                    │ Security Pipeline   │
                    │ Structured Clone    │
                    │ Style System        │
                    │ Layout System       │
                    └──────────┬──────────┘
                               │
             ┌─────────────────┼─────────────────┐
             │                 │                 │
             ▼                 ▼                 ▼
           fetch()            XHR             Workers
           Modules          Scripts        MessageChannel
```

### Mandatory rule

> Before creating API-specific infrastructure, inspect whether the behavior belongs to an existing or reusable subsystem.

Examples:

| New API          | Prefer Reusing                    |
| ---------------- | --------------------------------- |
| `fetch()`        | Resource Loader                   |
| XHR              | Fetch Core                        |
| ES Modules       | Resource Loader + Module Cache    |
| Workers          | Task Scheduler + Structured Clone |
| MessageChannel   | Structured Clone + Task Scheduler |
| History state    | Structured Clone                  |
| CSS updates      | Mutation Pipeline                 |
| Live collections | DOM versioning                    |
| Image loading    | Resource Loader                   |
| Script loading   | Resource Loader + Task Scheduler  |

---

## 3. Core Architectural Primitives

The following primitives should be considered foundational infrastructure.

---

### 3.1 DOM Mutation Pipeline

All DOM mutations should pass through centralized DOM mutation primitives.

The intended flow is:

```text
JavaScript API
      │
      ▼
DOM Mutation
      │
      ▼
Mutation Classification
      │
      ├── Collection invalidation
      ├── Selector invalidation
      ├── Style invalidation
      ├── Layout invalidation
      ├── Paint invalidation
      └── Accessibility invalidation
```

Avoid scattered code like:

```rust
element.set_attribute(...);

invalidate_layout();
invalidate_style();
clear_collection_cache();
clear_selector_cache();
```

inside multiple JavaScript bindings.

Prefer:

```rust
dom.set_attribute(node, name, value);
```

with centralized mutation processing.

---

### 3.2 Mutation Classification

Not every mutation requires a complete render pipeline invalidation.

Introduce explicit mutation metadata or derived dirty flags.

Example:

```rust
bitflags! {
    struct DirtyFlags: u32 {
        const DOM        = 1 << 0;
        const COLLECTION = 1 << 1;
        const SELECTORS  = 1 << 2;
        const STYLE      = 1 << 3;
        const LAYOUT     = 1 << 4;
        const PAINT      = 1 << 5;
        const A11Y       = 1 << 6;
    }
}
```

A mutation can then derive:

```text
setAttribute("class")
    ↓
DOM
COLLECTION
SELECTORS
STYLE
LAYOUT
PAINT
```

While:

```text
scrollTop change
    ↓
PAINT
```

And:

```text
text node update
    ↓
DOM
LAYOUT
PAINT
```

The system should avoid invalidating unrelated subsystems.

---

### 3.3 Lazy Invalidation

Prefer:

```text
Mutation
   ↓
Increment version / mark dirty
   ↓
Continue execution
   ↓
Recompute only when required
```

Over:

```text
Mutation
   ↓
Immediately rebuild
   ├── Collections
   ├── Selectors
   ├── Styles
   ├── Layout
   └── Paint
```

Example:

```rust
struct VersionedCache<T> {
    version: u64,
    value: T,
}
```

A cache should be recomputed when:

```rust
if cache.version != current_version {
    cache.recompute();
}
```

---

## 4. DOM Node Lifetime and Identity

### 4.1 Non-destructive Node Removal

Removing a node from the DOM should not immediately destroy the node.

A node can exist in different states:

```text
Alive + Connected
Alive + Detached
Destroyed
```

For example:

```rust
enum NodeState {
    Connected,
    Detached,
    Destroyed,
}
```

Expected behavior:

```javascript
const node = parent.removeChild(child);

// node must remain usable

otherParent.appendChild(node);
```

Desired implementation:

```text
removeChild()
     │
     ▼
Remove parent relationship
     │
     ▼
Mark node as detached
     │
     ▼
Preserve Node identity
     │
     ▼
Existing JavaScript references remain valid
```

Do not destroy a node merely because it is removed from the document tree.

---

### 4.2 Why Detached Nodes Matter

Correct detached-node behavior is foundational for:

* `removeChild`;
* `appendChild`;
* `insertBefore`;
* `replaceChild`;
* `replaceWith`;
* `DocumentFragment`;
* `cloneNode`;
* `adoptNode`;
* future Shadow DOM;
* future Range APIs;
* Selection APIs;
* drag-and-drop;
* template content;
* MutationObserver.

Implementing these features around destructive removal creates increasing complexity and future rewrites.

---

## 5. Node Identity Safety

If the DOM uses arena allocation and numeric `NodeId`s, avoid stale JavaScript wrappers becoming valid for a newly allocated node.

Instead of:

```text
NodeId = 42
```

prefer a generation-aware identity:

```rust
struct NodeHandle {
    id: NodeId,
    generation: u32,
}
```

Example:

```text
Node #42, generation 1
        │
        ▼
Node removed
        │
        ▼
Arena slot reused
        │
        ▼
Node #42, generation 2
```

An old JavaScript wrapper containing:

```text
(42, generation 1)
```

must not access:

```text
(42, generation 2)
```

This prevents stale object identity bugs.

---

## 6. JavaScript Native Object Lifetime

Avoid relying exclusively on manually clearing multiple caches such as:

```text
NODE_OBJECTS
CLASS_LIST_OBJECTS
DATASET_OBJECTS
ATTRIBUTES_OBJECTS
EVENT_LISTENERS
```

Instead, define a consistent native object lifecycle model.

Recommended direction:

```text
JS Object
    │
    ▼
Native Handle
    │
    ├── NodeHandle
    ├── DocumentHandle
    ├── CollectionHandle
    └── Host Object Handle
```

Native handles should validate:

1. object type;
2. context ownership;
3. node generation;
4. lifetime validity.

This reduces the amount of manual cache invalidation required by DOM operations.

---

## 7. Live DOM Collections

Do not implement live collections by eagerly rebuilding every collection after every DOM mutation.

Avoid:

```text
DOM mutation
    │
    ├── Update HTMLCollection A
    ├── Update HTMLCollection B
    ├── Update NodeList C
    └── Update NodeList D
```

Prefer lazy versioned collections.

Example:

```rust
struct LiveCollection {
    root: NodeId,
    query: CollectionQuery,
    last_dom_version: u64,
    cached_nodes: Vec<NodeHandle>,
}
```

On access:

```rust
if collection.last_dom_version != dom.version() {
    collection.recompute(dom);
}
```

This provides:

* live behavior;
* stable collection identity;
* lazy recomputation;
* reduced mutation cost.

---

### 7.1 Future Optimization: Subtree Versions

A global DOM version is a useful first step.

Later, optimize with subtree versions:

```text
Document
 ├── Header subtree version: 10
 ├── Main subtree version: 42
 └── Footer subtree version: 7
```

A collection rooted in:

```text
Main
```

does not need invalidation when:

```text
Footer
```

changes.

---

## 8. DOM Mutation Batching

A JavaScript loop such as:

```javascript
for (let i = 0; i < 10000; i++) {
    parent.appendChild(createElement("div"));
}
```

must not cause:

```text
10,000 style calculations
10,000 layouts
10,000 paints
```

Instead:

```text
JavaScript execution
        │
        ▼
DOM mutations accumulate
        │
        ▼
Microtask checkpoint
        │
        ▼
Style update
        │
        ▼
Layout
        │
        ▼
Paint
```

Conceptually:

```rust
begin_mutation_batch();

run_script();

end_mutation_batch();
```

This batching should ideally be automatic from page JavaScript's perspective.

---

## 9. Task Scheduler

Before implementing complex asynchronous APIs, establish a central task scheduler.

Suggested queues:

```text
Task Scheduler
├── Macrotasks
├── Microtasks
├── Timers
├── Network completions
├── Animation frames
├── Rendering tasks
└── Lifecycle tasks
```

Possible representation:

```rust
enum Task {
    Script(...),
    Timer(...),
    Network(...),
    AnimationFrame(...),
    Lifecycle(...),
}
```

Suggested execution flow:

```text
Select task
    │
    ▼
Execute task
    │
    ▼
Run JavaScript
    │
    ▼
Drain microtasks
    │
    ▼
Apply pending DOM work
    │
    ▼
Determine render opportunity
    │
    ▼
Run requestAnimationFrame
    │
    ▼
Style / Layout / Paint
```

This should become the basis for:

* Promises;
* timers;
* fetch;
* modules;
* async scripts;
* Workers;
* MessageChannel;
* lifecycle events.

---

## 10. Resource Loader

All resource loading should converge on a shared pipeline.

```text
Resource Request
       │
       ▼
URL Resolution
       │
       ▼
Security Policy
       │
       ▼
CSP
       │
       ▼
Mixed Content
       │
       ▼
Referrer Policy
       │
       ▼
Cache
       │
       ▼
Network
       │
       ▼
Decode
```

Suggested abstraction:

```rust
struct ResourceRequest {
    url: Url,
    initiator: Initiator,
    destination: Destination,
    priority: Priority,
    cors_mode: CorsMode,
    referrer_policy: ReferrerPolicy,
}
```

This pipeline should be reusable by:

* HTML;
* CSS;
* JavaScript scripts;
* ES modules;
* images;
* `fetch()`;
* XHR;
* future media APIs.

---

## 11. Fetch Architecture

Do not independently implement networking logic for:

* `fetch`;
* XHR;
* modules;
* future APIs.

Create a shared Fetch Core:

```text
Fetch Core
├── Request
├── Response
├── Headers
├── Body
└── Stream
```

Then expose adapters:

```text
Fetch Core
   │
   ├── fetch()
   ├── XMLHttpRequest
   ├── Module Loader
   └── Future APIs
```

This prevents duplicated implementations of:

* URL handling;
* headers;
* redirects;
* cancellation;
* body consumption;
* CORS;
* errors.

---

## 12. Structured Clone Before Workers

Do not implement Workers first.

Implement:

```text
1. Structured Clone
2. Transferable abstraction
3. MessagePort
4. MessageChannel
5. Worker
```

Structured Clone should be reusable for:

* `postMessage`;
* `MessageChannel`;
* Workers;
* `history.pushState`;
* `history.replaceState`;
* storage events;
* future cross-context messaging.

Avoid storing history state merely as a direct JavaScript object reference when the engine can instead use the shared clone mechanism.

---

## 13. Host State Modularization

Avoid allowing `HostState` to become a large container for unrelated features.

Do not converge toward:

```rust
struct HostState {
    // URL
    // history
    // CSP
    // permissions
    // layout
    // styles
    // navigation
    // trusted types
    // storage
    // ...
}
```

Prefer subsystem composition:

```rust
struct PageState {
    navigation: NavigationState,
    security: SecurityState,
    lifecycle: LifecycleState,
    layout: LayoutState,
    storage: StorageState,
}
```

Each subsystem should expose focused APIs:

```rust
page.navigation.navigate(...);
page.security.check_request(...);
page.lifecycle.transition(...);
page.layout.get_rect(...);
```

Benefits:

* reduced coupling;
* clearer ownership;
* easier testing;
* easier profiling;
* simpler future replacement.

---

## 14. CSS and Style Optimization

Before implementing extremely complex incremental layout, prioritize style caching.

Recommended stages:

```text
Stage 1
    Selector matching cache

Stage 2
    Computed style cache

Stage 3
    Dirty style propagation

Stage 4
    Layout invalidation optimization

Stage 5
    Incremental layout

Stage 6
    Paint damage tracking
```

---

### 14.1 Selector Matching Cache

Conceptually:

```text
(element, stylesheet_version)
        │
        ▼
Matched CSS Rules
```

Avoid repeatedly evaluating every selector when:

```text
nothing relevant changed
```

---

### 14.2 Computed Style Cache

Conceptually:

```text
(
    node,
    stylesheet_version,
    parent_style_version
)
        │
        ▼
ComputedStyle
```

---

### 14.3 Dirty Style Propagation

Example:

```text
Class changes
    │
    ▼
Target style dirty
```

Then determine whether descendants require invalidation.

If an inherited property changes:

```text
Target
 └── descendants may require recomputation
```

If a non-inherited property changes:

```text
Only target may need style recomputation
```

Avoid:

```text
Any style mutation
    ↓
Recompute entire document
```

when possible.

---

## 15. Typed CSS Properties

Avoid string-based CSS representations in layout hot paths.

Avoid:

```rust
HashMap<String, String>
```

during:

* layout;
* rendering;
* selector matching;
* style calculation.

Prefer:

```rust
enum PropertyId {
    Display,
    Width,
    Height,
    MarginTop,
    MarginRight,
    Color,
    Position,
    // ...
}
```

And typed values:

```rust
struct ComputedStyle {
    display: Display,
    width: Length,
    height: Length,
    position: Position,
    // ...
}
```

The pipeline should be:

```text
CSS text
    │
    ▼
Parser
    │
    ▼
Property IDs
    │
    ▼
Typed ComputedStyle
    │
    ▼
Layout
    │
    ▼
Paint
```

Not:

```text
Layout
    │
    ▼
HashMap<String, String>
```

---

## 16. String Interning and Atoms

Browser engines repeatedly process the same strings:

```text
div
span
input
class
id
style
href
display
position
```

Avoid repeated:

* heap allocations;
* string comparisons;
* hashing.

Introduce atoms or interned identifiers.

Example:

```rust
struct Atom(u32);
```

Possible internal usage:

```text
TagName
AttributeName
CSSProperty
ClassToken
```

The parser handles:

```text
String
```

and converts to:

```text
Atom / Enum / ID
```

The hot path should primarily use compact identifiers.

Benefits:

* faster comparisons;
* reduced allocations;
* reduced memory use;
* faster selector matching;
* faster DOM lookups.

---

## 17. Rendering Invalidation

The rendering pipeline should distinguish:

```text
Style dirty
Layout dirty
Paint dirty
Composite dirty
```

Example:

```text
Opacity animation
    ↓
Composite only

Color change
    ↓
Paint

Width change
    ↓
Layout + Paint

Display change
    ↓
Style + Layout + Paint
```

Do not treat all changes as equivalent.

Future architecture:

```text
Mutation
   │
   ▼
Style invalidation
   │
   ▼
Layout invalidation
   │
   ▼
Damage region generation
   │
   ▼
Paint only affected regions
   │
   ▼
Composite
```

---

## 18. Damage Tracking

Once paint invalidation exists, track affected regions.

Conceptually:

```rust
struct DamageRegion {
    rects: Vec<Rect>,
}
```

Or a more optimized representation later.

Example:

```text
Small button changes
    ↓
Invalidate button bounds
    ↓
Repaint affected region
```

Avoid:

```text
Small DOM update
    ↓
Repaint entire viewport
```

unless required.

---

## 19. JavaScript ↔ Rust Boundary Optimization

The JS/native boundary is likely to become a major performance hotspot.

Avoid repeated:

```text
JS value
    ↓
String conversion
    ↓
Rust String allocation
    ↓
DOM lookup
    ↓
Rust String allocation
    ↓
JS value
```

Optimize common paths.

Potential improvements:

* avoid temporary strings;
* use atoms;
* cache native handles;
* use typed conversions;
* avoid repeated prototype lookups;
* minimize FFI/native boundary crossings;
* batch operations where possible.

Benchmark separately:

```text
100k property reads
100k property writes
100k native function calls
100k event dispatches
```

---

## 20. Compatibility vs Performance

Maintain two separate development pipelines.

### Compatibility Pipeline

```text
Web API
   │
   ▼
Specification / behavior definition
   │
   ▼
Targeted WPT
   │
   ▼
Implementation
   │
   ▼
Regression tests
```

### Performance Pipeline

```text
Benchmark
   │
   ▼
Profile
   │
   ▼
Identify hotspot
   │
   ▼
Optimize
   │
   ▼
Measure before/after
   │
   ▼
Regression benchmark
```

Never accept an optimization solely because it:

> "looks faster"

Require measurable evidence.

---

## 21. Benchmark Suite

Maintain a dedicated benchmark suite.

---

### 21.1 DOM Benchmarks

```text
Create 1,000 nodes
Create 10,000 nodes
Append 10,000 nodes
Remove subtree
Move detached subtree
Clone subtree
querySelector
querySelectorAll
getElementsByClassName
innerHTML large tree
outerHTML replacement
```

---

### 21.2 JS ↔ Native Benchmarks

```text
100,000 DOM property reads
100,000 DOM property writes
100,000 native calls
100,000 event dispatches
classList mutations
dataset access
attribute access
```

---

### 21.3 CSS Benchmarks

```text
1,000 nodes / 100 rules
10,000 nodes / 500 rules
Deep selectors
Class mutations
Style mutations
Inherited property changes
```

---

### 21.4 Layout Benchmarks

```text
Static page
Single text mutation
Single class mutation
Viewport resize
Flex-heavy layout
Deep nested layout
Large list layout
```

---

### 21.5 Idle Game Benchmark

Because Nimble is optimized for lightweight and idle-game workloads, maintain a workload specifically representing the target use case.

Example:

```text
1,000 counters
Updates every 50ms
Frequent text mutations
Periodic class changes
Timers
Animation loop
Moderate DOM size
Long-running session
```

Measure:

```text
CPU usage
Frame time
Frame variance
Memory usage
Allocation rate
GC pressure
DOM update throughput
```

This benchmark should be considered more representative than generic browser benchmarks alone.

---

## 22. Profiling Requirements

Every significant optimization should be profiled.

Recommended workflow:

```text
Baseline benchmark
      │
      ▼
CPU profile
      │
      ▼
Memory profile
      │
      ▼
Flamegraph
      │
      ▼
Identify hot path
      │
      ▼
Optimize
      │
      ▼
Run same benchmark
      │
      ▼
Compare results
```

Record:

```text
Before
After
Percentage change
Regression risk
Affected workloads
```

Do not optimize based only on assumptions.

---

## 23. Script Loading Architecture

Do not implement:

* parser-blocking scripts;
* `defer`;
* `async`;
* modules;

as completely separate systems.

Use:

```text
HTML Parser
     │
     ▼
Script Discovery
     │
     ▼
Resource Loader
     │
     ▼
Script Scheduler
     │
     ├── Blocking
     ├── Async
     ├── Defer
     └── Module
```

The scheduler should determine execution order.

---

## 24. ES Module Architecture

Implement a reusable module system:

```text
Module URL
    │
    ▼
Resolver
    │
    ▼
Import Map
    │
    ▼
Resource Loader
    │
    ▼
Module Cache
    │
    ▼
Instantiation
    │
    ▼
Evaluation
```

The cache should be keyed by resolved module identity.

Avoid:

```text
Every import
    ↓
Fetch again
    ↓
Parse again
    ↓
Evaluate again
```

---

## 25. Security Pipeline

Security should not be implemented separately for every API.

Centralize checks.

```text
Operation
    │
    ▼
Origin Check
    │
    ▼
CORS
    │
    ▼
CSP
    │
    ▼
Permissions Policy
    │
    ▼
Secure Context
    │
    ▼
Operation
```

Examples:

```text
fetch()
Image loading
Module loading
Script loading
Clipboard
Notifications
Future APIs
```

should reuse centralized policy evaluation.

---

## 26. Avoid Permanent Fake APIs

Do not add silent no-op APIs merely to avoid exceptions.

Example:

```javascript
element.scrollIntoView();
```

A no-op implementation can cause application bugs that are difficult to diagnose.

If a compatibility stub is necessary:

1. explicitly classify it as a stub;
2. document it;
3. test it;
4. track it as incomplete;
5. avoid claiming the feature is implemented.

Feature states should be:

```text
Implemented
Partial
Compatibility Stub
Unsupported
```

Prefer semantically correct partial implementations over APIs that silently pretend to work.

---

## 27. Dataset and Exotic Property Behavior

Do not permanently accept API limitations solely because a current Rust binding does not expose a convenient mechanism.

Before declaring an API impossible or heavily simplified:

1. inspect the underlying QuickJS API;
2. determine whether the missing functionality exists upstream;
3. check whether a minimal binding can be added;
4. investigate property hooks or exotic objects;
5. only then accept a documented limitation.

This is especially relevant for:

* `dataset`;
* bracket access;
* live property reflection;
* DOM exotic objects;
* named properties;
* collection behavior.

A small missing native binding may unlock multiple Web APIs.

---

## 28. Web Platform Tests

WPT should begin before the engine reaches "platform readiness."

Start immediately with targeted subsets.

Recommended areas:

```text
DOM
Events
Selectors
URL
History
Timers
Fetch
Forms
Collections
```

Workflow:

```text
Implement feature
      │
      ▼
Run targeted WPT subset
      │
      ▼
Fix compatibility failures
      │
      ▼
Add Nimble regression tests
      │
      ▼
Track compatibility percentage
```

Do not wait until late development to discover that multiple APIs have incompatible semantics.

---

## 29. Automatic Capability Tracking

Where practical, capability reporting should derive from:

```text
Tests
+
Implementation status
```

rather than manual percentage estimates only.

Example:

```text
Feature:
    implemented: true
    unit_tests: true
    integration_tests: true
    WPT_pass_rate: 82%
    performance_benchmark: true
```

This makes roadmap progress measurable.

---

## 30. Recommended Implementation Priority

The following order prioritizes architecture, performance, and future reuse.

---

### P0 — Architectural Foundations

#### 1. Non-destructive detached DOM nodes

Implement:

```text
Connected
Detached
Destroyed
```

without destroying nodes on ordinary removal.

#### 2. Generation-safe Node handles

Use:

```text
(NodeId, Generation)
```

or equivalent.

#### 3. Central Mutation Pipeline

All DOM changes must use centralized invalidation.

#### 4. Dirty Flags

Classify:

```text
DOM
Collections
Selectors
Style
Layout
Paint
Accessibility
```

#### 5. Task Scheduler

Central event loop and task queues.

#### 6. Resource Loader

Shared loading pipeline.

#### 7. Structured Clone

Before Workers and MessageChannel.

---

## 31. P1 — Performance Infrastructure

#### 8. String Interning / Atoms

Intern:

```text
Tags
Attributes
CSS properties
Common identifiers
```

#### 9. Selector Cache

Cache selector matching.

#### 10. Computed Style Cache

Cache typed computed styles.

#### 11. Dirty Style Propagation

Avoid full document style recomputation.

#### 12. DOM Mutation Batching

Avoid layout after every mutation.

#### 13. Paint Damage Tracking

Track affected regions.

#### 14. JS ↔ Rust Boundary Benchmarks

Measure native binding costs.

---

## 32. P2 — High-Impact Compatibility

#### 15. URL

Implement:

* `URL`;
* `URLSearchParams`;
* relative resolution;
* base URL behavior.

#### 16. Fetch Core

Implement:

* Request;
* Response;
* Headers;
* Body;
* cancellation.

#### 17. AbortController

Shared cancellation primitive.

#### 18. Script Loading Modes

Implement:

* parser blocking;
* defer;
* async.

#### 19. ES Modules

Implement:

* resolver;
* import maps;
* dynamic import;
* module cache.

#### 20. Default Actions

Implement correct browser default behavior.

#### 21. Form Semantics

Expand real form behavior.

---

## 33. P3 — Rendering

#### 22. Real Scrolling

Implement:

* scroll position;
* scroll APIs;
* scrollIntoView;
* scroll offsets.

#### 23. Viewport

Implement real viewport state.

#### 24. Stacking Contexts

Implement stacking context behavior.

#### 25. z-index

Implement correct ordering.

#### 26. Transforms

Implement transform pipeline.

#### 27. Incremental Layout

After style invalidation infrastructure is stable.

#### 28. Compositing Layers

Where performance benefits justify it.

---

## 34. P4 — Parallelism

#### 29. MessagePort

#### 30. MessageChannel

#### 31. Structured Clone Integration

#### 32. Transferable Objects

#### 33. Worker

Implement only after the previous primitives exist.

---

## 35. P5 — Large Platform Features

#### 34. CSS Grid

#### 35. iframe

#### 36. Multiple Browsing Contexts

#### 37. Cross-window Messaging

#### 38. Shadow DOM

#### 39. Custom Elements

#### 40. MutationObserver

These features should be implemented on top of stable primitives rather than introducing parallel architecture.

---

## 36. Mandatory Implementation Rules

### Architecture-First Rule

Before implementing a new Web API:

```text
1. Identify existing shared primitives.
2. Identify duplicated logic.
3. Extend shared infrastructure when possible.
4. Avoid API-specific state machines.
```

---

### Hot-Path Rule

Do not use:

```text
Heap-allocated strings
Repeated parsing
Repeated selector matching
Repeated HashMap lookups
```

in hot paths when a:

```text
Atom
Enum
Interned ID
Typed representation
Cache
```

can be used.

---

### Mutation Rule

All DOM mutations must use centralized DOM primitives.

Do not manually invalidate unrelated caches from individual JavaScript bindings.

---

### Cache Rule

Caches must use explicit:

```text
Versioning
Dirty state
Dependency information
```

Prefer:

```text
Mutation
    ↓
Mark dirty
    ↓
Lazy recomputation
```

over eager global synchronization.

---

### Node Lifetime Rule

Removing a node from the document must not automatically destroy its identity.

JavaScript references to detached nodes should remain valid.

---

### Rendering Rule

Do not trigger style, layout, and paint after every individual DOM mutation.

Batch mutations and render at scheduler-defined render opportunities.

---

### Resource Rule

Resource-consuming APIs should converge on:

```text
URL resolution
    ↓
Security
    ↓
Policy checks
    ↓
Cache
    ↓
Network
    ↓
Decode
```

---

### Performance Rule

Every performance optimization must have:

```text
Baseline
Benchmark
Profile
Before result
After result
Regression check
```

---

### Testing Rule

Every significant feature should include:

1. unit tests;
2. integration tests when crossing crates;
3. compatibility/WPT tests where applicable;
4. regression tests;
5. performance tests for hot paths.

---

## 37. Definition of Done

A feature should not be considered fully complete merely because:

```text
The API exists
```

A stronger definition is:

```text
API implemented
    │
    ├── Semantics defined
    ├── Unit tested
    ├── Integration tested
    ├── Compatibility tested
    ├── Lifetime behavior verified
    ├── Invalidations verified
    ├── Error behavior verified
    ├── Performance measured if hot
    └── Regression tests added
```

---

## 38. Final Engineering Principle

The goal of Nimble should not be:

> Implement as many browser APIs as possible.

The goal should be:

> Build a small number of powerful, reusable, well-profiled browser-engine primitives from which many Web APIs can be implemented cheaply and correctly.

The preferred development model is:

```text
Shared Primitive
      │
      ▼
Multiple APIs
      │
      ▼
Compatibility Tests
      │
      ▼
Benchmarks
      │
      ▼
Profiling
      │
      ▼
Optimization
      │
      ▼
Regression Protection
```

A good implementation should make the next implementation easier.

If a new feature requires:

* another global cache;
* another invalidation mechanism;
* another scheduler;
* another network pipeline;
* another object lifetime model;

then first investigate whether the existing architecture should be generalized instead.

---

## Final Priority Summary

```text
P0 — Architecture
    ├── Detached node lifetime
    ├── Generation-safe handles
    ├── Mutation pipeline
    ├── Dirty flags
    ├── Task scheduler
    ├── Resource loader
    └── Structured clone

P1 — Performance
    ├── Atoms
    ├── Selector cache
    ├── Computed style cache
    ├── Dirty propagation
    ├── Mutation batching
    ├── Damage tracking
    └── JS/native profiling

P2 — Compatibility
    ├── URL
    ├── Fetch core
    ├── AbortController
    ├── Script modes
    ├── Modules
    ├── Default actions
    └── Forms

P3 — Rendering
    ├── Scrolling
    ├── Viewport
    ├── Stacking contexts
    ├── z-index
    ├── Transforms
    ├── Incremental layout
    └── Compositing

P4 — Parallelism
    ├── MessagePort
    ├── MessageChannel
    ├── Transferables
    └── Workers

P5 — Large Platform Features
    ├── Grid
    ├── iframe
    ├── Multiple browsing contexts
    ├── Shadow DOM
    ├── Custom Elements
    └── MutationObserver
```

> **Core principle: every implementation should either improve a shared primitive or reuse one. New APIs should not continuously create new architecture.**
