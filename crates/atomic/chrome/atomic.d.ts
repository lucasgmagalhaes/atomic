// Ambient type declarations for `globalThis.atomic` — the native bridge
// `apps/shell/src/chrome_bridge.rs`'s `register()` injects into every
// chrome bundle's JS context (toolbar.js, settings.js, add_profile.js,
// downloads_history.js). Every method here is a real `JS_NewCFunction2`
// binding backed by Rust, not a stub — see that file for the exact
// implementation each one queues into `AtomicApp::update`'s per-frame
// action drain.
//
// Editor-only: this workspace has no Node/npm build step (see
// CLAUDE.md's own note on why — cargo-husky stands in for tooling that
// would otherwise need it). Nothing compiles or type-checks this file at
// build time; it exists so an editor with TypeScript language-service
// support (VS Code, etc.) can autocomplete and catch typos in
// `atomic.*` calls while editing the chrome bundle .js files directly.
// Keep it in sync with `chrome_bridge.rs::register` by hand — nothing
// enforces that automatically.
//
// Every argument here is typed `number`/`string` because that's the real
// wire shape `chrome_bridge.rs`'s `read_js_number`/`read_js_string`
// helpers read (via `JS_ToCStringLen2` + `str::parse`, not a native
// float/bool binding) — passing `true`/`false` where a doc comment says
// "0/1" below will not do what you expect.

interface AtomicBridge {
  /** Opens the Add Profile modal. */
  addProfile(): void;
  /** Sets how many panes the active workspace shows at once. */
  setPaneCount(count: number): void;
  /** Switches the active workspace to the one at `index`. */
  switchWorkspace(index: number): void;
  /** Creates a new workspace and switches to it. */
  createWorkspace(): void;
  /** Sets the UI locale — `"en"` or `"pt"`; anything else falls back to `"en"`. */
  setLocale(locale: string): void;
  /**
   * Submits the Add Profile form. Field values are read directly off the
   * DOM by Rust (`#field-name`, `#field-start-url`, `#field-email`,
   * `#field-password`, `#field-proxy`) — this call takes no arguments.
   */
  createProfileSubmit(): void;
  /** Cancels/closes the Add Profile modal without creating a profile. */
  cancelAddProfile(): void;
  /** Adjusts the "max panes" setting by `delta` (negative decrements), clamped 1-6 host-side. */
  stepMaxPanes(delta: number): void;
  /** Adjusts the FPS cap by `delta` (negative decrements), clamped 1-60 host-side; no-op if the cap is currently disabled. */
  stepFpsCap(delta: number): void;
  /** Enables (`1`) or disables (`0`) the FPS cap. Any non-zero value counts as enabled. */
  setFpsCapEnabled(enabled: number): void;
  /** Switches the credential vault backing: `0` = plain file, non-zero = OS keychain. */
  setUseKeychain(useKeychain: number): void;
  /** Applies a GPU adapter selection by index; `-1` (or any negative number) selects the default adapter. */
  applyGpuAdapter(adapterIndex: number): void;
  /**
   * Adds a credential to the vault. Key/value are read directly off the
   * DOM by Rust (`#field-cred-key`, `#field-cred-value`) — this call
   * takes no arguments.
   */
  addCredential(): void;
  /** Removes the credential at `index` in the vault's current key list. */
  removeCredential(index: number): void;
  /** Imports browsing history from the configured Chrome profile into the selected pane. */
  importHistory(): void;
  /** Imports bookmarks from the configured Chrome profile. */
  importBookmarks(): void;
  /** Imports cookies from the configured Chrome profile into the selected pane. */
  importCookies(): void;
  /** Imports saved passwords from the configured Chrome profile. */
  importPasswords(): void;
  /** Closes the Settings window. */
  closeSettings(): void;

  // Not listed: downloading a URL from the downloads/history panel has no
  // `atomic.*` call of its own — `downloads_history.js`'s own click
  // handler on `#download-submit` is read by Rust straight off the
  // `#download-url` input's live `.value` (see
  // `ChromeAction::DownloadSubmitted`'s own doc for why), not pushed
  // through this bridge.
}

declare global {
  interface Window {
    atomic: AtomicBridge;
  }
  // eslint-disable-next-line no-var
  var atomic: AtomicBridge;
}

export {};
