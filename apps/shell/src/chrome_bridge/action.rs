//! `ChromeAction` — split out from `chrome_bridge.rs`.

#[derive(Debug, Clone)]
pub(crate) enum ChromeAction {
    AddProfile,
    SetPaneCount(usize),
    SwitchWorkspace(usize),
    CreateWorkspace,
    SetLocale(String),
    /// A download URL submitted from the downloads/history chrome panel.
    /// Unlike the other variants, this is never pushed from a native JS
    /// function's `argv` — the submitted `<input>`'s live `.value` is a
    /// `dom::Dom` field independent of any HTML attribute (see
    /// `dom::Dom::value`'s own doc), so `ChromeEngine::handle_click` reads
    /// it directly off the DOM before dispatching the click, and calls
    /// [`super::push_download_submitted`] itself instead of routing through
    /// a registered global function.
    DownloadSubmitted(String),
    CreateProfileSubmit,
    CancelAddProfile,
    StepMaxPanes(i32),
    StepFpsCap(i32),
    SetFpsCapEnabled(bool),
    SetUseKeychain(bool),
    /// `None` is the "Default" adapter, mirroring
    /// `PerformanceSettings::gpu_adapter`'s own `Option<usize>` shape.
    ApplyGpuAdapter(Option<usize>),
    /// The two credential fields are read directly off the DOM at click
    /// time, same reasoning [`ChromeAction::DownloadSubmitted`] documents —
    /// simpler than passing them back through JS argv.
    AddCredential,
    RemoveCredential(usize),
    ImportHistory,
    ImportBookmarks,
    ImportCookies,
    ImportPasswords,
    CloseSettings,
}
