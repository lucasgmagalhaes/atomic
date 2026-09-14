// Toolbar component — encapsulated as a function returning a real DOM
// node, mounted onto the shared `#root` container every chrome surface
// now uses (see `../../root.html`) instead of a bundle-specific top-level
// HTML element. Keeps its own `#toolbar` id so `toolbar.css`'s selectors
// need no change from before this component-folder split.
//
// Each button attaches its own real `click` listener at creation, so
// unlike a delegated `if/else` dispatch table there's nothing to keep in
// sync with the markup. Only `#workspaces`' contents change after load
// (`renderWorkspaces`, called by `ChromeEngine::sync_toolbar_state`) —
// same reason it stays a stable `<span>` container here instead of being
// rebuilt with the rest.
//
// Zero-argument `atomic.*` calls (`addProfile`, `createWorkspace`) are
// passed directly as `onClick` — no `() => atomic.x()` wrapper needed,
// since the native binding (registered with arity 0) ignores the click
// `Event` it'd otherwise receive as its first argument.
function Toolbar() {
  return Div(
    { id: "toolbar" },
    Span({ id: "workspaces" }),
    Span({ id: "label" }, "Panes:"),
    Button({ id: "count-1", onClick: () => atomic.setPaneCount(1) }, "1"),
    Button({ id: "count-2", onClick: () => atomic.setPaneCount(2) }, "2"),
    Button({ id: "count-4", onClick: () => atomic.setPaneCount(4) }, "4"),
    Button({ id: "count-6", onClick: () => atomic.setPaneCount(6) }, "6"),
    Button({ id: "add-profile", onClick: atomic.addProfile }, "+ Add Profile"),
    Button({ id: "locale-en", onClick: () => atomic.setLocale("en") }, "EN"),
    Button({ id: "locale-pt", onClick: () => atomic.setLocale("pt") }, "PT"),
  );
}

mount("root", Toolbar());

// Called by `ChromeEngine::sync_toolbar_state` with `[{name, active}, ...]`
// every time `AtomicApp`'s workspace list changes.
function renderWorkspaces(workspaces) {
  mount(
    "workspaces",
    workspaces.map((ws, i) =>
      Button({ id: `workspace-${i}`, active: ws.active, onClick: () => atomic.switchWorkspace(i) }, ws.name)
    ),
    Button({ id: "new-workspace", onClick: atomic.createWorkspace }, "+ New"),
  );
}
