# spec/ — Start Here

**Pivot (2026-09-16):** this project no longer builds its own rendering/JS engine (`neutron`, `atomicjs`). It now wraps **Chromium via CEF** (Chromium Embedded Framework) and focuses on the layer CEF doesn't give you for free: aggressive per-profile isolation, fast cold start, and low idle footprint at scale (many profiles/tabs open at once). See [ROADMAP.md](ROADMAP.md) for the phased plan and [RULES.md](RULES.md) before writing code.

The old engine's spec (JS runtime/DOM/CSS/layout/paint matrix, architecture playbook) is preserved for reference at [`spec-archive-neutron-engine/`](../spec-archive-neutron-engine/INDEX.md) — it's dead, not a live target. Don't pick up items from it.

## Reading order

1. **[ROADMAP.md](ROADMAP.md)** — phased CEF-integration plan, P0→P4.
2. **[RULES.md](RULES.md)** — mandatory rules before touching CEF binding code or process/isolation logic.
3. **[architecture/cef-integration.md](architecture/cef-integration.md)** — how the CEF binding layer is structured (multi-process model, IPC, the Rust↔C++ boundary).
4. **[architecture/isolation-and-perf.md](architecture/isolation-and-perf.md)** — the actual differentiator: per-profile process/data isolation, startup budget, idle throttling.
5. **[architecture/chrome-ui.md](architecture/chrome-ui.md)** — the app's own shell UI (toolbar/sidebar/settings/etc.), rendered by a dedicated CEF browser instead of `egui`, composited natively alongside per-profile panes.

## Directory map

```text
spec/
├── INDEX.md                      ← you are here
├── ROADMAP.md                    ← phased CEF-integration + perf/isolation plan
├── RULES.md                      ← mandatory rules + definition of done
└── architecture/
    ├── cef-integration.md        (CEF process model, Rust bindings, IPC to atomic shell)
    └── isolation-and-perf.md     (per-profile isolation, startup time, idle memory/CPU budget)
```

## What changed and why

See `CLAUDE.md`'s "Product positioning" and "Engine pivot" sections for the full reasoning. Short version: building a competitive rendering/JS engine from scratch (the old `neutron`/`atomicjs` plan) was the wrong bet — Chromium is a consolidated, battle-tested engine that already solves rendering/JS/compat correctly. The real differentiator this product can own is **what sits around** the engine: fast startup, low idle memory/CPU per profile, and strict default isolation between profiles — things stock Chromium-based browsers don't optimize for because they're not built for the many-isolated-profiles use case.

---

[← back to repo root CLAUDE.md](../CLAUDE.md)
