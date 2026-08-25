//! Performance settings — the "max live panes" and "frame cap" halves of
//! the mockup's "Settings: Performance (max live panes, background
//! throttling, GPU, frame cap)" spec gap. Pure client-side logic:
//! [`clamp_pane_count`](PerformanceSettings::clamp_pane_count) is what
//! `set_pane_count` actually clamps against; `fps_cap` is real state a
//! caller (`main.rs`) applies to every live pane's own worker process via
//! `profile::Profile::set_fps_cap` — genuine live throttling of the vsync
//! loop, not just a stored preference (see that method's own doc).
//!
//! Background throttling (pausing a hidden pane's own loop, see
//! `apps/shell/src/main.rs`'s `apply_visibility_throttling`) and GPU
//! selection (`gpu_adapter`, see that field's own doc) are both real now.
#[derive(Debug, PartialEq, Eq)]
pub struct PerformanceSettings {
    /// Caps how many panes the grid can hold at once (the mockup's 1/2/4/6
    /// toggle) - defaults to 6, the mockup's own largest preset, so the
    /// default behaves exactly like no cap at all.
    pub max_panes: usize,
    /// `Some(fps)` applies a real per-pane vsync cap (see
    /// `profile::Profile::set_fps_cap`) to every live pane and every pane
    /// spawned from now on; `None` (the default) leaves `profile-worker`'s
    /// own fixed 60fps loop untouched - genuinely uncapped, not "capped at
    /// 60" as a hidden default.
    pub fps_cap: Option<u32>,
    /// `Some(index)` opens that `render::list_adapters()`-enumerated GPU
    /// for every future pane spawn (see `profile::Profile::
    /// spawn_with_gpu_adapter`'s doc) - `None` (the default) uses
    /// `render::GpuRenderer::new`'s own default-adapter heuristic. Unlike
    /// `fps_cap`, this can't be applied live to an already-running pane
    /// (a process's GPU device is fixed for its lifetime) - changing it
    /// means respawning, same "spawn-time only" scope `proxy`/`dns` have.
    pub gpu_adapter: Option<usize>,
}

impl PerformanceSettings {
    pub fn new() -> Self {
        PerformanceSettings {
            max_panes: 6,
            fps_cap: None,
            gpu_adapter: None,
        }
    }

    /// Clamps `requested` into `1..=self.max_panes` - a shell should
    /// never show zero panes (nothing to click to add one back) or more
    /// than the configured cap.
    pub fn clamp_pane_count(&self, requested: usize) -> usize {
        requested.clamp(1, self.max_panes.max(1))
    }
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self::new()
    }
}
