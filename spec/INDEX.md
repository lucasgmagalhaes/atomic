# spec/ — Start Here

This directory replaces the old single `FULL_SPEC.md` (a 2,300-line file that had to be read in full to find anything). It's split so an agent picking up a task reads **only** the 2-3 small files relevant to that task, not the whole spec.

## Reading order

1. **[ROADMAP.md](ROADMAP.md)** — the actionable queue. Pick your next item here first; it tells you which detail files to read.
2. **[RULES.md](RULES.md)** — mandatory rules + definition of done. Short. Read before writing code, every time.
3. The specific `matrix/*.md` (current capability status + file references for the area you're touching) and/or `architecture/*.md` (the *how* — design rules for that subsystem) file(s) ROADMAP.md points you to.

Do not read every file in this directory for one task. Do not read `matrix/changelog.md` unless you need the historical "why was it built this way" detail — it's a large narrative log of already-completed work, not a task list.

## Directory map

```text
spec/
├── INDEX.md                          ← you are here
├── ROADMAP.md                        ← actionable P0-P5 queue, START HERE
├── RULES.md                          ← mandatory rules + definition of done
├── matrix/                           ← WHAT is done/needed, per area
│   ├── runtime.md                    (JS runtime & host — Promises, timers, modules, console)
│   ├── dom.md                        (DOM classes/properties/functions)
│   ├── events.md                     (Events & input)
│   ├── css-layout.md                 (HTML, CSS, layout)
│   ├── paint.md                      (Paint, canvas, compositing)
│   ├── browser-apis.md               (fetch/XHR, timers, URL, storage, files, media, device/UI — by API family)
│   ├── navigation-security.md        (window/location/history, origin/CORS/CSP, lifecycle)
│   └── changelog.md                  (large historical log — read only for "why", not "what's next")
└── architecture/                     ← HOW to build it (principles, not per-feature status)
    ├── overview.md                   (goals + the "architecture before APIs" rule)
    ├── primitives.md                 (mutation pipeline, node lifetime/identity, live collections,
    │                                   mutation batching, task scheduler, resource loader,
    │                                   fetch architecture, structured clone, host state)
    ├── performance.md                (style/selector caching, typed CSS, atoms, rendering
    │                                   invalidation, damage tracking, JS/Rust boundary,
    │                                   benchmark suite, profiling)
    ├── compatibility.md              (script loading, ES modules, security pipeline,
    │                                   avoid-fake-APIs policy, WPT, capability tracking)
    ├── priority-roadmap.md           (the raw P0-P5 list ROADMAP.md reconciles against reality)
    └── rules-and-dod.md              (full text behind RULES.md's summary)
```

## Overall progress (JS engine specifically — Part A of the old matrix)

Last computed 2026-08-25; see `matrix/*.md` for the file-level detail behind each number. Non-JS-engine crates (ipc/profile/net/security/platform-apis/apps-shell/workers/storage) are tracked in `CLAUDE.md`'s "Implementation status" instead, not here.

| Area | Progress | File |
| --- | ---: | --- |
| ECMAScript runtime | 75% | [matrix/runtime.md](matrix/runtime.md) |
| DOM / HTML APIs | 38% | [matrix/dom.md](matrix/dom.md) |
| CSS / layout | 48% | [matrix/css-layout.md](matrix/css-layout.md) |
| Paint / compositing | 42% | [matrix/paint.md](matrix/paint.md) |
| Browser APIs | 35% | [matrix/browser-apis.md](matrix/browser-apis.md) |
| Navigation / document lifecycle | ~~25%~~ higher, not recomputed | [matrix/navigation-security.md](matrix/navigation-security.md) |
| Standards / compatibility | 12% | [architecture/compatibility.md](architecture/compatibility.md) |

## Keeping this in sync

When you finish an item from ROADMAP.md:

1. Flip its `- [ ]` to `- [x]` (or `[~]`) in the relevant `matrix/*.md` file, with a one-line note + file reference (same convention already used throughout).
2. Update its line in `ROADMAP.md` too — they can drift, ROADMAP.md is the reconciled summary, matrix files are the detailed source.
3. Small doc-update commit, separate from the code commit (`docs: mark <item> done`) — see `.opencode/worker-prompt.md` for the exact convention an automated worker follows.

Never overwrite one of these files wholesale — edit the specific line/section you touched.

---

Also relevant, outside `spec/`: `CLAUDE.md` (repo-wide conventions, non-JS-engine crate status, native-binding gotchas), `mockup/browser-idle-spec.md` (product/crate-level roadmap), `mockup/rendering-engine-gaps.md` (rendering-engine gap map).
