// Settings component — encapsulated as a function returning a real DOM
// node, mounted onto the shared `#root` container (see `../../root.html`).
// Keeps its own `#settings` id so `settings.css`'s selectors need no
// change from before this component-folder split. Same reasoning
// `toolbar.js` documents: each button owns its own real `click` listener,
// so `ChromeEngine`'s `dispatch_click` (a real `dispatchEvent("click")` on
// the resolved element) reaches it directly, no delegated root listener
// needed, and zero-argument `atomic.*` calls are passed as `onClick`
// directly instead of wrapped in an arrow function.
//
// `apply-gpu` and `close-settings` intentionally have no `onClick` here —
// neither was ever wired to an `atomic.*` call in the original
// hand-written `settings.js`.
function Settings() {
  return Div(
    { id: "settings" },
    Div({ className: "section-title" }, "Performance"),
    Row(
      {},
      Label({}, "Max live panes"),
      Button({ id: "panes-minus", onClick: () => atomic.stepMaxPanes(-1) }, "-"),
      Span({ id: "panes-value" }, "1"),
      Button({ id: "panes-plus", onClick: () => atomic.stepMaxPanes(1) }, "+"),
    ),
    Row(
      {},
      Label({}, "Cap frame rate"),
      Button({ id: "fps-cap-off", onClick: () => atomic.setFpsCapEnabled(0) }, "Off"),
      Button({ id: "fps-cap-on", onClick: () => atomic.setFpsCapEnabled(1) }, "On"),
      Button({ id: "fps-minus", onClick: () => atomic.stepFpsCap(-1) }, "-"),
      Span({ id: "fps-value" }, "-"),
      Button({ id: "fps-plus", onClick: () => atomic.stepFpsCap(1) }, "+"),
    ),
    Div({ id: "error", className: "error" }),
    Div({ className: "section-title" }, "GPU adapter"),
    Div({ id: "adapter-list" }),
    Button({ id: "apply-gpu" }, "Apply (respawns all panes)"),
    Div({ className: "section-title" }, "Credentials"),
    Row(
      {},
      Label({}, "Use OS keychain"),
      Button({ id: "keychain-off", onClick: () => atomic.setUseKeychain(0) }, "Off"),
      Button({ id: "keychain-on", onClick: () => atomic.setUseKeychain(1) }, "On"),
    ),
    Div({ id: "vault-error", className: "error" }),
    Row(
      {},
      Input({ id: "field-cred-key", type: "text", placeholder: "key" }),
      Input({ id: "field-cred-value", type: "text", placeholder: "value" }),
      Button({ id: "add-credential", onClick: atomic.addCredential }, "Add"),
    ),
    Div({ id: "credential-list" }),
    Div({ className: "section-title" }, "Import from Chrome"),
    Row(
      {},
      Button({ id: "import-history", onClick: atomic.importHistory }, "Import History"),
      Button({ id: "import-bookmarks", onClick: atomic.importBookmarks }, "Import Bookmarks"),
      Button({ id: "import-cookies", onClick: atomic.importCookies }, "Import Cookies"),
      Button({ id: "import-passwords", onClick: atomic.importPasswords }, "Import Passwords"),
    ),
    Div({ id: "import-result" }),
    Div({ id: "bookmark-list" }),
    Button({ id: "close-settings" }, "Close"),
  );
}

mount("root", Settings());

// Called by `ChromeEngine::sync_settings_state` with the adapter name list
// plus the currently-selected index (`null` = default) — replaces
// `#adapter-list`'s children, folding the old "active-if-none" special
// case for the default button into a plain `active` prop like every other
// button here.
function renderAdapters(adapters, selected) {
  mount(
    "adapter-list",
    Button({ id: "adapter-default", active: selected === null, onClick: () => atomic.applyGpuAdapter(-1) }, "Default"),
    adapters.map((name, i) =>
      Button({ id: `adapter-${i}`, active: selected === i, onClick: () => atomic.applyGpuAdapter(i) }, name)
    ),
  );
}

// Called with the vault's current credential key list.
function renderCredentials(keys) {
  mount(
    "credential-list",
    List({
      items: keys,
      emptyText: "No stored credentials.",
      renderItem: (key, i) =>
        Row({}, Span({}, key), Button({ id: `cred-remove-${i}`, onClick: () => atomic.removeCredential(i) }, "Remove")),
    }),
  );
}

// Called with already-formatted bookmark display lines — no empty-state
// message, same as the `bookmark_html` it replaces (which just mapped an
// empty array to an empty string).
function renderBookmarks(lines) {
  mount(
    "bookmark-list",
    lines.map((line) => Row({}, line)),
  );
}
