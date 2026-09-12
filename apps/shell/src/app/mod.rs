//! `NimbleApp`, the `eframe::App` implementation that is the whole GUI shell.
//! Split into one file per cohesive responsibility (SRP), same convention
//! this workspace's other large-file splits use (`js-runtime`'s
//! `dom_bindings`/`form_data`, `profile`'s `profile_worker`, `dom`'s own
//! module split):
//! - [`pane`]: `Pane` (one live grid cell) and its spawn helpers.
//! - [`resource_overlay`]: the CPU/RAM/FPS sparkline drawn on each pane.
//! - [`panes`]: pane lifecycle (`NimbleApp` methods) — spawn/close/duplicate,
//!   pane-count changes, per-pane proxy/FPS/GPU application, address bar
//!   navigation, and the per-pane download box.
//! - [`workspaces`]: workspace switching/creation and moving a pane between
//!   workspaces, plus background-throttling hidden panes.
//! - [`automation_engine`]: the app-lifetime automation engine — running a script,
//!   rebinding it to the current live panes, and ticking it every frame.
//! - [`vault`]: the credential vault (open/switch backing/add/remove).
//! - [`chrome_import_actions`]: the "Import from Chrome" buttons (history,
//!   bookmarks, cookies, passwords).
//! - [`update`]: the `eframe::App::update` entry point, which just calls
//!   into the per-panel drawing methods below in order.
//! - [`toolbar_ui`]/[`automation_ui`]/[`add_profile_ui`]/[`settings_ui`]/
//!   [`side_panel_ui`]/[`grid_ui`]: one file per UI panel `update` draws —
//!   the toolbar (workspace switcher, pane-count buttons, address bar,
//!   proxy box), the automation script box, the "Add Profile" modal, the
//!   Settings window, the Downloads & History side panel, and the pane grid
//!   itself (click/scroll/keyboard routing, context menu, texture paint).
//!
//! Every field on [`NimbleApp`] stays private to this crate and is reached
//! directly from every submodule above (they're descendants of this
//! module), matching how the `dom` crate's own split reaches `Dom`'s
//! private fields from its sibling submodules.

mod add_profile_ui;
mod automation_engine;
mod automation_ui;
mod chrome_add_profile_ui;
mod chrome_downloads_history_ui;
mod chrome_import_actions;
mod chrome_settings_ui;
mod chrome_toolbar_ui;
mod grid_ui;
mod pane;
mod panes;
mod resource_overlay;
mod settings_ui;
mod side_panel_ui;
mod toolbar_ui;
mod update;
mod vault;
mod workspaces;

use shell::i18n::Locale;
use shell::settings::PerformanceSettings;
use shell::workspace::WorkspaceManager;

use pane::Pane;

pub(crate) const PANE_WIDTH: u32 = 512;
pub(crate) const PANE_HEIGHT: u32 = 320;

pub(crate) struct NimbleApp {
    panes: Vec<Pane>,
    /// Index into `panes` the address bar / proxy box / Reload button
    /// target - set by clicking a pane in the grid. Clamped back into
    /// range whenever the pane count shrinks.
    selected: usize,
    address_bar_text: String,
    /// The proxy text box's current contents - not necessarily what the
    /// selected pane is actually using, since a proxy only takes effect on
    /// [`panes::apply_proxy`] (it can't be changed on an already-running
    /// profile - see `BrowserView::spawn_with_proxy`'s doc). Starts empty
    /// (no proxy).
    proxy_text: String,
    /// Named groups of profiles (mockup's workspace switcher). Real data
    /// layer, still no switcher UI here (see `workspace::WorkspaceManager`'s
    /// own doc) - its role in this GUI is naming each grid pane so an
    /// automation script can address it via `pane("pane-N")`.
    workspace: WorkspaceManager,
    automation_script: String,
    /// `Ok(result)` from the last automation script's top-level eval, or
    /// `Err(message)` if it raised. `None` before any script has run.
    automation_result: Option<Result<String, String>>,
    /// App-lifetime automation engine - genuinely never dropped for as
    /// long as `NimbleApp` runs, so `every`/`on`/`cron` callbacks a script
    /// registers via `run_automation_script` keep firing on every
    /// subsequent `tick_automation_engine` call, closing the automation
    /// crate's own long-documented "ephemeral per run" gap. Borrows a
    /// `js_runtime::Runtime` that's `Box::leak`ed once at startup (see
    /// `Default::default`) - a real, deliberate leak, not a bug: this
    /// runtime needs to outlive `AutomationEngine`, which needs to live as
    /// long as `NimbleApp` itself, and `NimbleApp` isn't guaranteed not to
    /// move after construction (`eframe` owns it behind a `Box<dyn App>`)
    /// - leaking is the same "give it a stable heap address, app-lifetime,
    /// no cleanup needed before process exit" trade this workspace's
    /// native quickjs bindings already make for their own opaque pointers,
    /// just via a safe `Box::leak` instead of raw allocation.
    automation_engine: automation::AutomationEngine<'static>,
    /// The active UI language - see `shell::i18n`. Toggled via the EN/PT
    /// buttons in the toolbar, applied immediately (every string is looked
    /// up fresh each `update()` frame, so there's no restart/re-render step
    /// needed).
    locale: Locale,
    /// The download-URL text box in the Downloads & History side panel -
    /// targets the selected pane on submit, same "selected pane" scoping
    /// the address bar/proxy box already use.
    download_url_text: String,
    performance: PerformanceSettings,
    /// `Some` once the Settings window has actually opened the real vault
    /// (`vault_ui::open`) - not eagerly at startup, so a session that
    /// never opens Settings never touches the filesystem for it.
    vault: Option<security::vault::CredentialVault>,
    vault_error: Option<String>,
    settings_open: bool,
    /// New-credential form fields in the Settings window's Credentials
    /// section.
    vault_key_text: String,
    vault_value_text: String,
    /// Set by a failed `profile::Profile::set_fps_cap` call - independent
    /// of `vault_error`, shown in the same Settings window.
    performance_error: Option<String>,
    /// Which backing `ensure_vault_open` should use - real OS keychain
    /// (Windows Credential Manager, via `vault_ui::open_with_keychain`) or
    /// the plain-file-key vault (`vault_ui::open`, the default). A
    /// distinct vault file per mode (see `vault_ui::open_with_keychain`'s
    /// doc) - toggling this is switching vaults, not migrating one.
    vault_use_keychain: bool,
    /// `Ok(message)`/`Err(message)` from the last "Import from Chrome"
    /// button click - either history or bookmarks, whichever ran last.
    import_result: Option<Result<String, String>>,
    /// Real bookmarks pulled from the last successful "Import Bookmarks"
    /// click - shown read-only in Settings (no bookmarks feature/UI
    /// exists elsewhere in this shell to hand them to yet, see
    /// `chrome_import`'s own doc on scope).
    imported_bookmarks: Vec<shell::chrome_import::Bookmark>,
    /// The mockup's "Add profile modal" (name, start URL, email/password
    /// autofill, proxy) - see [`AddProfileForm`] and
    /// `panes::create_profile`. `None` when the modal isn't open; a
    /// fresh `AddProfileForm::default()` is created each time it opens so
    /// a previous attempt's typed values don't linger into the next one.
    add_profile_form: Option<AddProfileForm>,
    add_profile_error: Option<String>,
    /// Track B spike: the toolbar rendered by Nimble's own engine instead
    /// of egui (see `crate::chrome_engine`) — additive next to the real
    /// `draw_toolbar` for now, not a replacement, so nothing regresses
    /// while the architecture proves itself. `chrome_gpu` is this chrome
    /// surface's own GPU context, same "each render surface opens its own"
    /// convention `profile-worker` already follows for pane content.
    chrome_toolbar: crate::chrome_engine::ChromeEngine<'static>,
    chrome_gpu: render::GpuRenderer,
    chrome_texture: Option<egui::TextureHandle>,
    /// Second chrome surface, same reasoning as `chrome_toolbar` — its own
    /// `Runtime`/`Context` (one per chrome surface, mirroring one
    /// `profile-worker`-style page per tab), additive next to the real
    /// `draw_downloads_history_panel` for now.
    chrome_downloads_history: crate::chrome_engine::ChromeEngine<'static>,
    chrome_downloads_texture: Option<egui::TextureHandle>,
    /// Third chrome surface — the Add Profile modal, same reasoning as the
    /// other two. `chrome_add_profile_was_open` tracks the closed->open
    /// transition so form fields are prefilled/reset exactly once per open
    /// (see `chrome_add_profile_ui`'s own doc).
    chrome_add_profile: crate::chrome_engine::ChromeEngine<'static>,
    chrome_add_profile_texture: Option<egui::TextureHandle>,
    chrome_add_profile_was_open: bool,
    /// Fourth chrome surface — the Settings window, same reasoning as the
    /// other three.
    chrome_settings: crate::chrome_engine::ChromeEngine<'static>,
    chrome_settings_texture: Option<egui::TextureHandle>,
}

/// One in-progress "Add Profile" modal's form fields - see the mockup's
/// own "Add profile modal" (name, start URL, email/password autofill,
/// proxy, launch on start). "Launch on start" isn't included: it implies
/// a persisted list of profiles to auto-spawn on shell startup, and
/// nothing in this shell persists *which* profiles existed across a
/// restart yet (each launch starts with exactly one default pane) - real
/// scope not attempted here, not silently faked as a checkbox that does
/// nothing.
#[derive(Default)]
struct AddProfileForm {
    name: String,
    start_url: String,
    email: String,
    password: String,
    proxy: String,
}

impl Default for NimbleApp {
    fn default() -> Self {
        let mut workspace = WorkspaceManager::new();
        let panes = vec![pane::spawn_pane(&mut workspace, "pane-1".to_string())];

        // See `automation_engine`'s own doc for why this leak is
        // deliberate: the runtime must outlive an app-lifetime engine,
        // and `NimbleApp` isn't guaranteed a stable address of its own.
        let automation_runtime: &'static js_runtime::Runtime =
            Box::leak(Box::new(js_runtime::Runtime::new()));
        let automation_engine =
            automation::AutomationEngine::new(automation_runtime, std::collections::HashMap::new());

        NimbleApp {
            panes,
            selected: 0,
            address_bar_text: String::new(),
            proxy_text: String::new(),
            workspace,
            automation_script: String::new(),
            automation_result: None,
            automation_engine,
            locale: Locale::En,
            download_url_text: String::new(),
            performance: PerformanceSettings::new(),
            vault: None,
            vault_error: None,
            settings_open: false,
            vault_key_text: String::new(),
            vault_value_text: String::new(),
            performance_error: None,
            vault_use_keychain: false,
            import_result: None,
            imported_bookmarks: Vec::new(),
            add_profile_form: None,
            add_profile_error: None,
            // Same deliberate leak `automation_engine` above already
            // documents: this runtime must outlive the chrome engine, and
            // `NimbleApp` isn't guaranteed a stable address of its own.
            chrome_toolbar: crate::chrome_engine::ChromeEngine::new_toolbar(Box::leak(Box::new(
                js_runtime::Runtime::new(),
            ))),
            chrome_gpu: render::GpuRenderer::new(),
            chrome_texture: None,
            chrome_downloads_history: crate::chrome_engine::ChromeEngine::new_downloads_history(
                Box::leak(Box::new(js_runtime::Runtime::new())),
            ),
            chrome_downloads_texture: None,
            chrome_add_profile: crate::chrome_engine::ChromeEngine::new_add_profile(Box::leak(
                Box::new(js_runtime::Runtime::new()),
            )),
            chrome_add_profile_texture: None,
            chrome_add_profile_was_open: false,
            chrome_settings: crate::chrome_engine::ChromeEngine::new_settings(Box::leak(Box::new(
                js_runtime::Runtime::new(),
            ))),
            chrome_settings_texture: None,
        }
    }
}
